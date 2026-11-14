//! « Mode Blender » de l'éditeur de bâtiment (bedit.rs) : déplacer des sommets, extruder des faces.
//!
//! - Un déplacement de sommet est gardé comme (position d'origine → nouvelle position), dans le repère du modèle du
//!   jeu. Il s'applique à tous les sommets dont la position d'origine est à moins de `MOVE_MATCH` : les sommets
//!   doublés d'une même position (coutures d'UV, parties voisines) et ceux des niveaux de détail plus grossiers
//!   qui tombent au même endroit bougent ensemble.
//! - Une extrusion est gardée comme la liste des centres d'origine des triangles de la face (comme les triangles
//!   retirés), la normale de la face et une distance (négative : la face s'enfonce). La face avance le long de sa
//!   normale ; chaque bord libre de la face reçoit une face latérale (même partie, donc même texture, UV étirées
//!   le long du bord) qui ferme le volume. Seul le niveau de détail le plus fin est extrudé.
//! - Ordre d'application, dans la vue comme à l'export : triangles retirés (d'après leur centre d'origine),
//!   sommets déplacés, extrusions (triangles reconnus par leur centre d'origine, avant déplacement), ouvertures.
//!
//! Le cœur travaille sur les maillages « en vrac » d'openings.rs (GMesh) : le même code sert au modèle de la vue,
//! à la collision et au .ydr.xml de CodeWalker.

use crate::bedit::RemovedSet;
use crate::gta::drawable::RModel;
use crate::math::{v3, V3};
use crate::openings::{pos_of, GMesh, Layout, Vx};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Un sommet déplacé (repère du modèle du jeu).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VertexMove {
    /// position d'origine
    pub from: [f32; 3],
    /// nouvelle position ; pour un sommet créé par une extrusion (`ex` > 0) : `to − from` est le décalage ajouté
    /// à sa position extrudée
    pub to: [f32; 3],
    /// 0 : sommets du modèle (et tout ce qui en est copié) ; n : seulement le dessus créé par l'extrusion n
    /// (numérotée à partir de 1), repéré par la position d'origine du sommet dont il est la copie
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ex: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Une face extrudée.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Extrusion {
    /// centre d'origine de chaque triangle de la face
    pub tris: Vec<[f32; 3]>,
    /// normale de la face (côté visible au moment du choix)
    pub nrm: [f32; 3],
    /// distance le long de la normale (m, repère du modèle)
    pub dist: f32,
}

/// Tolérance pour reconnaître un sommet déplacé par sa position d'origine (m), y compris dans les niveaux de
/// détail plus grossiers — **estimé**.
pub const MOVE_MATCH: f32 = 0.02;

/// Deux sommets à moins de cette distance sont au même endroit (bords libres d'une face, faces voisines) — **estimé**.
pub const WELD_EPS: f32 = 0.001;

pub fn a3(v: V3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

pub fn v3a(a: [f32; 3]) -> V3 {
    v3(a[0], a[1], a[2])
}

/// Déplacements rangés par case de 10 cm.
pub struct MoveSet {
    cells: HashMap<(i32, i32, i32), Vec<(V3, V3)>>,
}

const MS_CELL: f32 = 0.1;

fn cell(v: V3) -> (i32, i32, i32) {
    ((v.x / MS_CELL).floor() as i32, (v.y / MS_CELL).floor() as i32, (v.z / MS_CELL).floor() as i32)
}

impl MoveSet {
    /// Les déplacements des sommets du modèle (`ex` = 0).
    pub fn new(list: &[VertexMove]) -> MoveSet {
        MoveSet::layer(list, 0)
    }

    /// Les déplacements d'une couche : 0 pour les sommets du modèle, n pour le dessus de l'extrusion n.
    pub fn layer(list: &[VertexMove], ex: u32) -> MoveSet {
        let mut cells: HashMap<(i32, i32, i32), Vec<(V3, V3)>> = HashMap::new();
        for m in list.iter().filter(|m| m.ex == ex) {
            let f = v3a(m.from);
            cells.entry(cell(f)).or_default().push((f, v3a(m.to)));
        }
        MoveSet { cells }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Déplacement le plus proche d'un sommet de position d'origine `p` : (position d'origine, nouvelle position).
    fn find(&self, p: V3) -> Option<(V3, V3)> {
        let (x, y, z) = cell(p);
        let mut best: Option<(f32, V3, V3)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for (f, t) in self.cells.get(&(x + dx, y + dy, z + dz)).into_iter().flatten() {
                        let d = (*f - p).len();
                        if d <= MOVE_MATCH && best.map(|b| d < b.0).unwrap_or(true) {
                            best = Some((d, *f, *t));
                        }
                    }
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }

    /// Nouvelle position d'un sommet dont la position d'origine est `p` (le déplacement le plus proche), ou rien.
    pub fn get(&self, p: V3) -> Option<V3> {
        self.find(p).map(|b| b.1)
    }

    /// Décalage (nouvelle position − position d'origine) du déplacement le plus proche de `p`, ou rien.
    pub fn delta(&self, p: V3) -> Option<V3> {
        self.find(p).map(|(f, t)| t - f)
    }
}

fn set_pos(l: &Layout, v: &mut Vx, p: V3) {
    v[l.pos] = p.x;
    v[l.pos + 1] = p.y;
    v[l.pos + 2] = p.z;
}

fn set_nrm(l: &Layout, v: &mut Vx, n: V3) {
    if let Some(k) = l.nrm {
        v[k] = n.x;
        v[k + 1] = n.y;
        v[k + 2] = n.z;
    }
}

/// Centre de chaque triangle d'un maillage.
pub fn tri_centers(m: &GMesh) -> Vec<V3> {
    m.idx
        .chunks_exact(3)
        .map(|t| {
            let p = |k: u32| m.verts.get(k as usize).map(|v| pos_of(&m.layout, v)).unwrap_or(V3::ZERO);
            (p(t[0]) + p(t[1]) + p(t[2])) * (1.0 / 3.0)
        })
        .collect()
}

/// Déplace les sommets du modèle (tous les maillages ; seulement les déplacements `ex` = 0). Rend le nombre de
/// sommets déplacés.
pub fn apply_moves(meshes: &mut [GMesh], moves: &[VertexMove]) -> usize {
    let ms = MoveSet::new(moves);
    if ms.is_empty() {
        return 0;
    }
    let mut n = 0;
    for m in meshes.iter_mut() {
        let l = m.layout.clone();
        for v in m.verts.iter_mut() {
            if let Some(t) = ms.get(pos_of(&l, v)) {
                set_pos(&l, v, t);
                n += 1;
            }
        }
    }
    n
}

/// Positions soudées : chaque point reçoit le numéro du premier point déjà vu à moins de `WELD_EPS` (cases de la
/// taille de la tolérance, cases voisines comprises) — deux sommets à 0,4 mm l'un de l'autre sont le même point
/// même s'ils tombent de part et d'autre d'un arrondi.
pub struct Weld {
    cells: HashMap<(i32, i32, i32), Vec<(V3, u32)>>,
    next: u32,
}

impl Weld {
    pub fn new() -> Weld {
        Weld { cells: HashMap::new(), next: 0 }
    }

    fn key(p: V3) -> (i32, i32, i32) {
        ((p.x / WELD_EPS).floor() as i32, (p.y / WELD_EPS).floor() as i32, (p.z / WELD_EPS).floor() as i32)
    }

    /// Numéro du point `p` (nouveau s'il n'y a rien d'assez proche).
    pub fn id(&mut self, p: V3) -> u32 {
        let (x, y, z) = Weld::key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(l) = self.cells.get(&(x + dx, y + dy, z + dz)) {
                        if let Some((_, id)) = l.iter().find(|(q, _)| (*q - p).len() <= WELD_EPS) {
                            return *id;
                        }
                    }
                }
            }
        }
        let id = self.next;
        self.next += 1;
        self.cells.entry((x, y, z)).or_default().push((p, id));
        id
    }
}

impl Default for Weld {
    fn default() -> Self {
        Weld::new()
    }
}

/// Tangente (x, y, z) d'un triangle d'après ses positions et ses UV, et le sens des UV par rapport à la normale
/// `n` (+1 ou −1). Rien si les UV sont dégénérées.
fn uv_tangent(p: [V3; 3], uv: [(f32, f32); 3], n: V3) -> Option<(V3, f32)> {
    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
    let (du1, dv1, du2, dv2) = (uv[1].0 - uv[0].0, uv[1].1 - uv[0].1, uv[2].0 - uv[0].0, uv[2].1 - uv[0].1);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-10 {
        return None;
    }
    let r = 1.0 / det;
    let t = (e1 * dv2 - e2 * dv1) * r;
    let b = (e2 * du1 - e1 * du2) * r;
    // tangente rendue perpendiculaire à la normale
    let t = t - n * n.dot(t);
    if t.len() < 1e-8 {
        return None;
    }
    let t = t.norm();
    Some((t, if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 }))
}

/// `extrude_tris_ex` sans la liste des sommets ajoutés (tests).
#[cfg(test)]
pub fn extrude_tris(m: &mut GMesh, sel: &[usize], n: V3, d: f32) -> usize {
    extrude_tris_ex(m, sel, n, d).0
}

/// Extrude les triangles `sel` (indices de triangles) d'un maillage de `d` le long de `n` : leurs sommets sont
/// doublés et avancés, chaque bord libre (bord qu'un seul triangle choisi porte, sommets comparés par position à
/// `WELD_EPS` près) reçoit une face latérale (normale, tangente et UV recalculées). Rend le nombre de triangles
/// ajoutés et, pour chaque sommet ajouté (dans l'ordre), le sommet dont il est la copie et s'il est sur le dessus
/// de l'extrusion (face avancée et haut des faces latérales).
pub fn extrude_tris_ex(m: &mut GMesh, sel: &[usize], n: V3, d: f32) -> (usize, Vec<(u32, bool)>) {
    let l = m.layout.clone();
    let nv = m.verts.len();
    let ok = |t: usize| t * 3 + 2 < m.idx.len() && m.idx[t * 3..t * 3 + 3].iter().all(|&k| (k as usize) < nv);
    let sel: Vec<usize> = sel.iter().copied().filter(|&t| ok(t)).collect();
    let mut created: Vec<(u32, bool)> = Vec::new();
    if sel.is_empty() || d.abs() < 1e-6 {
        return (0, created);
    }
    let off = n * d;
    // sens de la face : celui de la plupart de ses triangles (normale géométrique sommée)
    let tri_g = |t: usize| {
        let p = |j: usize| pos_of(&l, &m.verts[m.idx[t * 3 + j] as usize]);
        (p(1) - p(0)).cross(p(2) - p(0))
    };
    let face_g = sel.iter().fold(V3::ZERO, |a, &t| a + tri_g(t));
    // bords dans le sens des triangles (inversé pour un triangle tourné à contresens), comptés sans sens ; les
    // sommets sont reconnus par position soudée (pas par arrondi : pas de « nageoires » sur un bord à cheval)
    let mut weld = Weld::new();
    let mut edges: Vec<(u32, u32, usize)> = Vec::new();
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for &t in &sel {
        let k = [m.idx[t * 3], m.idx[t * 3 + 1], m.idx[t * 3 + 2]];
        let flip = tri_g(t).dot(face_g) < 0.0;
        for i in 0..3 {
            let (a, b) = (k[i], k[(i + 1) % 3]);
            let (a, b) = if flip { (b, a) } else { (a, b) };
            let (ka, kb) = (weld.id(pos_of(&l, &m.verts[a as usize])), weld.id(pos_of(&l, &m.verts[b as usize])));
            if ka == kb {
                continue;
            }
            *count.entry(if ka < kb { (ka, kb) } else { (kb, ka) }).or_default() += 1;
            edges.push((a, b, t));
        }
    }
    // pour chaque bord : sa clé soudée (avant que les sommets ne soient doublés)
    let edge_keys: Vec<(u32, u32)> = edges.iter().map(|&(a, b, _)| {
        let (ka, kb) = (weld.id(pos_of(&l, &m.verts[a as usize])), weld.id(pos_of(&l, &m.verts[b as usize])));
        if ka < kb { (ka, kb) } else { (kb, ka) }
    }).collect();
    // convention du signe de la tangente du modèle : comparée au sens des UV de la face d'origine
    let tan_conv = |t: usize, mm: &GMesh| -> f32 {
        let (Some(u), Some(tk)) = (l.uv, l.tan) else { return 1.0 };
        let vs = [0, 1, 2].map(|j| &mm.verts[mm.idx[t * 3 + j] as usize]);
        let p = vs.map(|v| pos_of(&l, v));
        let g = (p[1] - p[0]).cross(p[2] - p[0]);
        let nn = l.nrm.map(|k| v3(vs[0][k], vs[0][k + 1], vs[0][k + 2])).filter(|v| v.len() > 1e-6).unwrap_or(g).norm();
        match uv_tangent(p, vs.map(|v| (v[u], v[u + 1])), nn) {
            Some((_, h)) => {
                let w = vs[0][tk + 3];
                if w * h < 0.0 { -1.0 } else { 1.0 }
            }
            None => 1.0,
        }
    };
    let convs: Vec<f32> = edges.iter().map(|&(_, _, t)| tan_conv(t, m)).collect();
    // la face avance : sommets doublés
    let mut top: HashMap<u32, u32> = HashMap::new();
    for &t in &sel {
        for j in 0..3 {
            let k = m.idx[t * 3 + j];
            let nk = *top.entry(k).or_insert_with(|| {
                let mut v = m.verts[k as usize].clone();
                let p = pos_of(&l, &v);
                set_pos(&l, &mut v, p + off);
                m.verts.push(v);
                created.push((k, true));
                m.verts.len() as u32 - 1
            });
            m.idx[t * 3 + j] = nk;
        }
    }
    // faces latérales sur les bords libres
    let mut added = 0;
    for (ei, &(a, b, _)) in edges.iter().enumerate() {
        if count.get(&edge_keys[ei]).copied().unwrap_or(0) != 1 {
            continue;
        }
        let (va, vb) = (m.verts[a as usize].clone(), m.verts[b as usize].clone());
        let (pa, pb) = (pos_of(&l, &va), pos_of(&l, &vb));
        // face latérale (a, b, b', a') : elle prend le bord dans le sens de la face, l'arête voisine le prend dans
        // l'autre sens, le volume reste orienté ; sa normale est sa normale géométrique (dehors si la face sort,
        // vers le creux si elle s'enfonce)
        let side = (pb - pa).cross(off).norm();
        // UV (toutes les coordonnées de texture) : celles du bord, puis décalées perpendiculairement au bord, à la
        // même échelle que le long du bord
        let (mut ta, mut tb) = (va.clone(), vb.clone());
        let elen = (pb - pa).len();
        for u in l.uv.iter().chain(l.uvx.iter()).copied() {
            let du = (vb[u] - va[u], vb[u + 1] - va[u + 1]);
            let ulen = (du.0 * du.0 + du.1 * du.1).sqrt();
            if ulen > 1e-6 && elen > 1e-6 {
                let s = d.abs() * ulen / elen;
                let perp = (-du.1 / ulen * s, du.0 / ulen * s);
                for v in [&mut ta, &mut tb] {
                    v[u] += perp.0;
                    v[u + 1] += perp.1;
                }
            }
        }
        set_pos(&l, &mut ta, pa + off);
        set_pos(&l, &mut tb, pb + off);
        let mut quad = [va, vb, tb, ta];
        let pts = [pa, pb, pb + off, pa + off];
        // tangente et binormale recalculées pour la face latérale (normale et UV nouvelles) : d'après ses UV, ou à
        // défaut le long du bord
        let (tan, sign) = l
            .uv
            .and_then(|u| uv_tangent([pts[0], pts[1], pts[2]], [0, 1, 2].map(|j| (quad[j][u], quad[j][u + 1])), side))
            .map(|(t, h)| (t, h * convs[ei]))
            .unwrap_or_else(|| ((pb - pa).norm(), l.tan.map(|k| if quad[0][k + 3] < 0.0 { -1.0 } else { 1.0 }).unwrap_or(1.0)));
        for v in quad.iter_mut() {
            set_nrm(&l, v, side);
            if let Some(k) = l.tan {
                v[k] = tan.x;
                v[k + 1] = tan.y;
                v[k + 2] = tan.z;
                v[k + 3] = sign;
            }
            if let Some(k) = l.bin {
                let bn = side.cross(tan) * sign;
                v[k] = bn.x;
                v[k + 1] = bn.y;
                v[k + 2] = bn.z;
                v[k + 3] = sign;
            }
        }
        let base = m.verts.len() as u32;
        m.verts.extend(quad);
        created.extend([(a, false), (b, false), (b, true), (a, true)]);
        // sens des triangles : la normale géométrique regarde du côté voulu (comme les embrasures d'openings.rs)
        let g = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        if g.dot(side) >= 0.0 {
            m.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            m.idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
        added += 2;
    }
    (added, created)
}

/// Applique déplacements puis extrusions à une liste de maillages. `primary` : les maillages du niveau de détail
/// le plus fin (les seuls extrudés). Les déplacements `ex` = n s'appliquent au dessus créé par l'extrusion n,
/// juste après elle. Rend (sommets déplacés, triangles extrudés).
pub fn apply_edits(meshes: &mut [GMesh], primary: &[usize], moves: &[VertexMove], extrusions: &[Extrusion]) -> (usize, usize) {
    // centres d'origine, avant déplacement : c'est par eux que les extrusions reconnaissent leurs triangles ;
    // positions d'origine de chaque sommet : c'est par elles que les sommets créés par une extrusion sont repérés
    let centers: Vec<Vec<V3>> = meshes.iter().enumerate().map(|(i, m)| if primary.contains(&i) && !extrusions.is_empty() { tri_centers(m) } else { Vec::new() }).collect();
    let mut origin: Vec<Vec<V3>> = meshes.iter().enumerate().map(|(i, m)| if primary.contains(&i) && !extrusions.is_empty() { m.verts.iter().map(|v| pos_of(&m.layout, v)).collect() } else { Vec::new() }).collect();
    let mut moved = apply_moves(meshes, moves);
    let mut extruded = 0;
    for (ei, e) in extrusions.iter().enumerate() {
        if e.dist.abs() < 1e-4 || e.tris.is_empty() {
            continue;
        }
        let set = RemovedSet::new(&e.tris);
        let n = v3a(e.nrm).norm();
        let lm = MoveSet::layer(moves, ei as u32 + 1);
        for &mi in primary {
            let Some(m) = meshes.get_mut(mi) else { continue };
            let sel: Vec<usize> = centers[mi].iter().enumerate().filter(|(_, c)| set.contains(**c)).map(|(i, _)| i).collect();
            if sel.is_empty() {
                continue;
            }
            extruded += sel.len();
            let start = m.verts.len();
            let (_, created) = extrude_tris_ex(m, &sel, n, e.dist);
            let l = m.layout.clone();
            for (j, (src, is_top)) in created.into_iter().enumerate() {
                let o = origin[mi].get(src as usize).copied().unwrap_or(V3::ZERO);
                origin[mi].push(o);
                if is_top {
                    if let Some(dl) = lm.delta(o) {
                        let v = &mut m.verts[start + j];
                        let p = pos_of(&l, v);
                        set_pos(&l, v, p + dl);
                        moved += 1;
                    }
                }
            }
        }
    }
    (moved, extruded)
}

/// Applique déplacements et extrusions à un modèle de la vue (toutes ses parties : un seul niveau de détail).
pub fn edit_rmodel(m: &mut RModel, moves: &[VertexMove], extrusions: &[Extrusion]) -> (usize, usize) {
    if moves.is_empty() && extrusions.is_empty() {
        return (0, 0);
    }
    let mut meshes = crate::openings::rmodel_to_meshes(m);
    let primary: Vec<usize> = (0..meshes.len()).collect();
    let r = apply_edits(&mut meshes, &primary, moves, extrusions);
    crate::openings::meshes_to_rmodel(m, &meshes);
    r
}

/// Applique déplacements (tous les niveaux de détail) et extrusions (niveau le plus fin) à un .ydr.xml de
/// CodeWalker. Rend le XML, les sommets déplacés et les triangles extrudés ; rien si rien n'a changé.
pub fn apply_to_ydr_xml(src: &str, moves: &[VertexMove], extrusions: &[Extrusion]) -> Option<(String, usize, usize)> {
    if moves.is_empty() && extrusions.is_empty() {
        return None;
    }
    let x = crate::openings::read_ydr_xml(src)?;
    let primary: Vec<usize> = x.geoms.iter().enumerate().filter(|(_, g)| g.primary).map(|(i, _)| i).collect();
    let mut meshes: Vec<GMesh> = x.geoms.iter().map(|g| g.mesh.clone()).collect();
    let (moved, extruded) = apply_edits(&mut meshes, &primary, moves, extrusions);
    if moved == 0 && extruded == 0 {
        return None;
    }
    Some((crate::openings::write_ydr_xml(&x, &meshes), moved, extruded))
}

// ------------------------------------------------------------------ choix d'une face

/// Cosinus de l'angle maximal entre deux triangles d'une même face plane (4°) — **estimé**.
pub const PLANAR_COS: f32 = 0.9976;
/// Écart maximal d'un triangle au plan de la face (m) — **estimé**.
pub const PLANAR_DIST: f32 = 0.02;

/// Normale géométrique (non normée) d'un triangle.
pub fn tri_normal(t: &[V3; 3]) -> V3 {
    (t[1] - t[0]).cross(t[2] - t[0])
}

/// Face plane autour du triangle `start` : triangles voisins (`neighbors`) de proche en proche, dans le plan du
/// premier (même sens de normale, à 4° et 2 cm près). `pos` : positions d'un triangle ; `ok` : triangle utilisable.
pub fn planar_region(start: usize, pos: &dyn Fn(usize) -> [V3; 3], neighbors: &dyn Fn(usize, &mut Vec<usize>), ok: &dyn Fn(usize) -> bool) -> Vec<usize> {
    let t0 = pos(start);
    let n0 = tri_normal(&t0).norm();
    let c0 = (t0[0] + t0[1] + t0[2]) * (1.0 / 3.0);
    let planar = |t: &[V3; 3]| {
        let g = tri_normal(t);
        let on_plane = t.iter().all(|p| (*p - c0).dot(n0).abs() <= PLANAR_DIST);
        on_plane && (g.len() < 1e-8 || g.norm().dot(n0) >= PLANAR_COS)
    };
    let mut seen: std::collections::HashSet<usize> = [start].into_iter().collect();
    let mut stack = vec![start];
    let mut out = Vec::new();
    let mut nb = Vec::new();
    while let Some(t) = stack.pop() {
        out.push(t);
        nb.clear();
        neighbors(t, &mut nb);
        for &k in &nb {
            if seen.insert(k) && ok(k) && planar(&pos(k)) {
                stack.push(k);
            }
        }
    }
    out
}

/// Face plane d'un maillage autour du triangle `start` (voisins : sommets à la même position, à `WELD_EPS` près).
pub fn mesh_planar_face(m: &GMesh, start: usize) -> Vec<usize> {
    let tp = |t: usize| -> [V3; 3] { [0, 1, 2].map(|j| m.verts.get(m.idx[t * 3 + j] as usize).map(|v| pos_of(&m.layout, v)).unwrap_or(V3::ZERO)) };
    let mut weld = Weld::new();
    let ids: Vec<[u32; 3]> = (0..m.idx.len() / 3).map(|t| tp(t).map(|p| weld.id(p))).collect();
    let mut by_pos: HashMap<u32, Vec<usize>> = HashMap::new();
    for (t, k) in ids.iter().enumerate() {
        for id in k {
            by_pos.entry(*id).or_default().push(t);
        }
    }
    planar_region(start, &tp, &|t, out| {
        for id in &ids[t] {
            out.extend(by_pos.get(id).into_iter().flatten());
        }
    }, &|_| true)
}

/// Normale moyenne (pondérée par l'aire) d'une face.
pub fn face_normal(tris: &[[V3; 3]]) -> V3 {
    tris.iter().fold(V3::ZERO, |a, t| a + tri_normal(t)).norm()
}

/// Ligne de commande de test : `dragowalker --edit-ydr-xml <entrée.ydr.xml> <sortie.ydr.xml> extrude x y z distance`
/// (face plane du niveau fin autour du triangle le plus proche de x y z) ou `... move x y z dx dy dz` (sommet le
/// plus proche).
pub fn cli_edit_xml(args: &[String]) {
    const USAGE: &str = "usage : --edit-ydr-xml <entrée.ydr.xml> <sortie.ydr.xml> extrude x y z distance | move x y z dx dy dz";
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else { return println!("{USAGE}") };
    let need = match args.get(2).map(|s| s.as_str()) {
        Some("move") => 9,
        Some("extrude") => 7,
        _ => return println!("attendu : move ou extrude\n{USAGE}"),
    };
    let nums: Vec<Option<f32>> = (3..need).map(|i| args.get(i).and_then(|s| s.parse::<f32>().ok())).collect();
    if let Some(k) = nums.iter().position(|v| v.is_none()) {
        return println!("nombre manquant ou illisible (argument {}) : {:?}\n{USAGE}", k + 4, args.get(k + 3).map(|s| s.as_str()).unwrap_or(""));
    }
    let f = |i: usize| nums.get(i - 3).copied().flatten().unwrap_or(0.0);
    let Ok(src) = std::fs::read_to_string(input) else { return println!("lecture impossible : {input}") };
    let Some(x) = crate::openings::read_ydr_xml(&src) else { return println!("XML illisible") };
    for s in &x.skipped {
        println!("laissée telle quelle : {s}");
    }
    let at = v3(f(3), f(4), f(5));
    let (mut moves, mut ex) = (Vec::new(), Vec::new());
    match args.get(2).map(|s| s.as_str()) {
        Some("move") => {
            let near = x.geoms.iter().flat_map(|g| g.mesh.verts.iter().map(|v| pos_of(&g.mesh.layout, v))).min_by(|a, b| (*a - at).len().total_cmp(&(*b - at).len()));
            let Some(p) = near else { return println!("aucun sommet") };
            moves.push(VertexMove { from: a3(p), to: a3(p + v3(f(6), f(7), f(8))), ex: 0 });
            println!("sommet {p:?} déplacé de {:?}", v3(f(6), f(7), f(8)));
        }
        Some("extrude") => {
            let mut best: Option<(f32, usize, usize)> = None;
            for (gi, g) in x.geoms.iter().enumerate().filter(|(_, g)| g.primary) {
                for (ti, c) in tri_centers(&g.mesh).iter().enumerate() {
                    let d = (*c - at).len();
                    if best.map(|b| d < b.0).unwrap_or(true) {
                        best = Some((d, gi, ti));
                    }
                }
            }
            let Some((_, gi, ti)) = best else { return println!("aucun triangle") };
            let m = &x.geoms[gi].mesh;
            let face = mesh_planar_face(m, ti);
            let centers = tri_centers(m);
            let tp = |t: usize| -> [V3; 3] { [0, 1, 2].map(|j| m.verts.get(m.idx[t * 3 + j] as usize).map(|v| pos_of(&m.layout, v)).unwrap_or(V3::ZERO)) };
            let n = face_normal(&face.iter().map(|&t| tp(t)).collect::<Vec<_>>());
            println!("face de {} triangle(s), partie {gi}, normale {n:?}", face.len());
            ex.push(Extrusion { tris: face.iter().map(|&t| a3(centers[t])).collect(), nrm: a3(n), dist: f(6) });
        }
        _ => return println!("attendu : move ou extrude\n{USAGE}"),
    }
    match apply_to_ydr_xml(&src, &moves, &ex) {
        Some((out, nm, ne)) => match std::fs::write(output, out) {
            Ok(()) => println!("{nm} sommet(s) déplacé(s), {ne} triangle(s) extrudé(s), écrit dans {output}"),
            Err(e) => println!("écriture impossible : {output} ({e})"),
        },
        None => println!("rien n'a changé"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkey(v: V3) -> (i32, i32, i32) {
        ((v.x * 1000.0).round() as i32, (v.y * 1000.0).round() as i32, (v.z * 1000.0).round() as i32)
    }

    fn lay() -> Layout {
        // position, normale, uv
        Layout { int: vec![false; 8], pos: 0, nrm: Some(3), uv: Some(6), ..Default::default() }
    }

    /// Cube unité [0, 1]³, sommets partagés, triangles tournés vers l'extérieur (normale géométrique sortante).
    fn cube() -> GMesh {
        let mut verts = Vec::new();
        for i in 0..8 {
            let (x, y, z) = ((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32);
            verts.push(vec![x, y, z, 0.0, 0.0, 1.0, x, y]);
        }
        // (a, b, c, d) dans le sens direct vu de l'extérieur
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut idx = Vec::new();
        for q in quads {
            idx.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
        }
        GMesh { layout: lay(), verts, idx }
    }

    fn tris(m: &GMesh) -> Vec<[V3; 3]> {
        m.idx.chunks_exact(3).map(|t| [0, 1, 2].map(|j| pos_of(&m.layout, &m.verts[t[j] as usize]))).collect()
    }

    /// Volume signé (théorème de la divergence) : positif si les triangles regardent dehors.
    fn volume(m: &GMesh) -> f32 {
        tris(m).iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum()
    }

    /// Chaque arête (par position) est portée par exactement deux triangles, une fois dans chaque sens.
    fn closed(m: &GMesh) -> bool {
        let mut c: HashMap<((i32, i32, i32), (i32, i32, i32)), i32> = HashMap::new();
        for t in tris(m) {
            for i in 0..3 {
                *c.entry((pkey(t[i]), pkey(t[(i + 1) % 3]))).or_default() += 1;
            }
        }
        c.iter().all(|(&(a, b), &n)| n == 1 && c.get(&(b, a)) == Some(&1))
    }

    fn top_face(m: &GMesh) -> Vec<usize> {
        tri_centers(m).iter().enumerate().filter(|(_, c)| (c.z - 1.0).abs() < 1e-4).map(|(i, _)| i).collect()
    }

    #[test]
    fn le_cube_de_test_est_ferme() {
        let m = cube();
        assert!((volume(&m) - 1.0).abs() < 1e-5);
        assert!(closed(&m));
    }

    #[test]
    fn extruder_le_dessus_ferme_le_volume() {
        let mut m = cube();
        let sel = top_face(&m);
        assert_eq!(sel.len(), 2);
        let added = extrude_tris(&mut m, &sel, v3(0.0, 0.0, 1.0), 0.5);
        assert_eq!(added, 8, "4 bords libres → 4 faces latérales de 2 triangles");
        assert!(closed(&m), "le volume doit rester fermé");
        assert!((volume(&m) - 1.5).abs() < 1e-4, "volume {}", volume(&m));
        // les faces latérales regardent dehors (normale géométrique et normale des sommets)
        for t in &tris(&m)[12..] {
            let c = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
            let out = v3(c.x - 0.5, c.y - 0.5, 0.0);
            assert!(tri_normal(t).dot(out) > 0.0);
        }
        for v in &m.verts[m.verts.len() - 16..] {
            let out = v3(v[0] - 0.5, v[1] - 0.5, 0.0);
            assert!(v3(v[3], v[4], v[5]).dot(out) > 0.0, "normale de sommet tournée vers l'intérieur : {v:?}");
        }
        // UV étirées : le haut d'une face latérale est décalé d'autant que la hauteur (0,5) à l'échelle du bord
        let side = &m.verts[m.verts.len() - 4..];
        let (b, t) = (&side[0], &side[3]);
        let duv = ((t[6] - b[6]).powi(2) + (t[7] - b[7]).powi(2)).sqrt();
        assert!((duv - 0.5).abs() < 1e-4, "décalage d'UV {duv}");
    }

    #[test]
    fn enfoncer_une_face_creuse_le_volume() {
        let mut m = cube();
        let sel = top_face(&m);
        extrude_tris(&mut m, &sel, v3(0.0, 0.0, 1.0), -0.3);
        assert!(closed(&m));
        assert!((volume(&m) - 0.7).abs() < 1e-4, "volume {}", volume(&m));
    }

    #[test]
    fn une_face_tournee_dans_l_autre_sens_s_extrude_aussi() {
        // le dessus du cube, pris avec la normale vers le bas et une distance négative : même résultat
        let mut m = cube();
        let sel = top_face(&m);
        extrude_tris(&mut m, &sel, v3(0.0, 0.0, -1.0), -0.5);
        assert!(closed(&m));
        assert!((volume(&m) - 1.5).abs() < 1e-4);
    }

    #[test]
    fn deplacer_un_sommet_entraine_ses_doubles_et_les_niveaux_de_detail() {
        // deux parties qui partagent le coin (1, 1, 1) (sommet doublé), et un « niveau de détail » à 1 cm près
        let mut a = cube();
        let mut b = cube();
        for v in b.verts.iter_mut() {
            v[0] += 1.0;
        }
        let mut lod = GMesh { layout: lay(), verts: vec![vec![1.008, 1.0, 0.995, 0.0, 0.0, 1.0, 0.0, 0.0], vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0], vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]], idx: vec![0, 1, 2] };
        let uv_before = a.verts[7][6];
        let mut ms = vec![std::mem::replace(&mut a, cube()), std::mem::replace(&mut b, cube()), std::mem::replace(&mut lod, cube())];
        let mv = [VertexMove { from: [1.0, 1.0, 1.0], to: [1.2, 1.0, 1.5], ex: 0 }];
        let n = apply_moves(&mut ms, &mv);
        // cube a : sommet 7 ; cube b : sommet 6 (0+1, 1, 1) ; lod : sommet 0
        assert_eq!(n, 3);
        assert_eq!(&ms[0].verts[7][..3], &[1.2, 1.0, 1.5]);
        assert_eq!(&ms[1].verts[6][..3], &[1.2, 1.0, 1.5]);
        assert_eq!(&ms[2].verts[0][..3], &[1.2, 1.0, 1.5]);
        // les autres sommets et les attributs ne bougent pas
        assert_eq!(ms[0].verts[7][6], uv_before);
        assert_eq!(&ms[0].verts[6][..3], &[0.0, 1.0, 1.0]);
        assert_eq!(&ms[1].verts[7][..3], &[2.0, 1.0, 1.0]);
        // trop loin (3 cm) : pas reconnu
        let mut far = vec![GMesh { layout: lay(), verts: vec![vec![1.03, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]], idx: vec![] }];
        assert_eq!(apply_moves(&mut far, &mv), 0);
    }

    #[test]
    fn extrusion_reconnue_par_les_centres_d_origine_apres_deplacement() {
        // le coin (1, 1, 1) est monté de 20 cm, puis le dessus extrudé : les triangles du dessus sont retrouvés par
        // leurs centres d'origine, et le volume reste fermé
        let mut ms = vec![cube()];
        let centers: Vec<[f32; 3]> = top_face(&ms[0]).iter().map(|&t| a3(tri_centers(&ms[0])[t])).collect();
        let mv = [VertexMove { from: [1.0, 1.0, 1.0], to: [1.0, 1.0, 1.2], ex: 0 }];
        let ex = [Extrusion { tris: centers, nrm: [0.0, 0.0, 1.0], dist: 1.0 }];
        let (moved, extruded) = apply_edits(&mut ms, &[0], &mv, &ex);
        assert_eq!((moved, extruded), (1, 2));
        assert!(closed(&ms[0]));
        // le coin déplacé et extrudé : (1, 1, 2.2)
        assert!(tris(&ms[0]).iter().flatten().any(|p| (*p - v3(1.0, 1.0, 2.2)).len() < 1e-5));
    }

    #[test]
    fn face_plane_autour_d_un_triangle() {
        let m = cube();
        let start = top_face(&m)[0];
        let mut f = mesh_planar_face(&m, start);
        f.sort();
        assert_eq!(f, top_face(&m));
        let tp: Vec<[V3; 3]> = f.iter().map(|&t| tris(&m)[t]).collect();
        assert!((face_normal(&tp) - v3(0.0, 0.0, 1.0)).len() < 1e-5);
    }

    const XML: &str = "<Drawable>
 <BoundingBoxMin x=\"0\" y=\"0\" z=\"0\" />
 <BoundingBoxMax x=\"1\" y=\"1\" z=\"0\" />
 <ShaderGroup>
 </ShaderGroup>
 <DrawableModelsHigh>
  <Item>
   <Geometries>
    <Item>
     <BoundingBoxMin x=\"0\" y=\"0\" z=\"0\" w=\"0\" />
     <BoundingBoxMax x=\"1\" y=\"1\" z=\"0\" w=\"0\" />
     <VertexBuffer>
      <Layout type=\"GTAV1\">
       <Position />
       <Normal />
       <Colour0 />
       <TexCoord0 />
      </Layout>
      <Data>
       0 0 0   0 0 1   255 0 0 255   0 0
       1 0 0   0 0 1   255 0 0 255   1 0
       1 1 0   0 0 1   255 0 0 255   1 1
       0 1 0   0 0 1   255 0 0 255   0 1
      </Data>
     </VertexBuffer>
     <IndexBuffer>
      <Data>
       0 1 2 0 2 3
      </Data>
     </IndexBuffer>
    </Item>
   </Geometries>
  </Item>
 </DrawableModelsHigh>
 <DrawableModelsMedium>
  <Item>
   <Geometries>
    <Item>
     <VertexBuffer>
      <Layout type=\"GTAV1\">
       <Position />
       <Normal />
       <Colour0 />
       <TexCoord0 />
      </Layout>
      <Data>
       0 0 0   0 0 1   255 0 0 255   0 0
       1.01 0 0   0 0 1   255 0 0 255   1 0
       0 1 0   0 0 1   255 0 0 255   0 1
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
 </DrawableModelsMedium>
</Drawable>";

    #[test]
    fn fichier_xml_deplacement_et_extrusion() {
        // le coin (1, 0, 0) descend de 10 cm dans les deux niveaux de détail ; le carré du niveau fin est extrudé de
        // 2 m vers le haut : 4 faces latérales, boîte de la partie et du modèle agrandies
        let mv = [VertexMove { from: [1.0, 0.0, 0.0], to: [1.0, 0.0, -0.1], ex: 0 }];
        let ex = [Extrusion { tris: vec![[2.0 / 3.0, 1.0 / 3.0, 0.0], [1.0 / 3.0, 2.0 / 3.0, 0.0]], nrm: [0.0, 0.0, 1.0], dist: 2.0 }];
        let (out, moved, extruded) = apply_to_ydr_xml(XML, &mv, &ex).expect("xml");
        assert_eq!((moved, extruded), (2, 2));
        let x = crate::openings::read_ydr_xml(&out).expect("relu");
        assert_eq!(x.geoms.len(), 2);
        let (hi, med) = (&x.geoms[0].mesh, &x.geoms[1].mesh);
        assert!(x.geoms[0].primary && !x.geoms[1].primary);
        assert_eq!(hi.idx.len() / 3, 2 + 8);
        assert_eq!(med.idx.len() / 3, 1, "le niveau moyen n'est pas extrudé");
        assert!(med.verts.iter().any(|v| (pos_of(&med.layout, v) - v3(1.0, 0.0, -0.1)).len() < 1e-5), "sommet du niveau moyen déplacé");
        assert!(hi.verts.iter().any(|v| (pos_of(&hi.layout, v) - v3(1.0, 0.0, 1.9)).len() < 1e-5));
        // couleurs entières, indices valides
        assert!(out.contains("255 0 0 255"));
        assert!(hi.idx.iter().all(|&k| (k as usize) < hi.verts.len()));
        // boîtes agrandies (z jusqu'à 2)
        assert!(out.contains("<BoundingBoxMax x=\"1\" y=\"1\" z=\"2\" w=\"0\" />"), "{out}");
        assert!(out.lines().nth(2).unwrap().contains("z=\"2\""), "en-tête : {}", out.lines().nth(2).unwrap());
        // rien à faire : rien
        assert!(apply_to_ydr_xml(XML, &[], &[]).is_none());
    }
    #[test]
    fn le_dessus_d_une_extrusion_se_deplace_sans_le_bas() {
        // le dessus du cube extrudé de 1 m ; le coin (1, 1, 1) du dessus (couche 1) monte de 50 cm, le bas reste
        let mut ms = vec![cube()];
        let centers: Vec<[f32; 3]> = top_face(&ms[0]).iter().map(|&t| a3(tri_centers(&ms[0])[t])).collect();
        let ex = [Extrusion { tris: centers, nrm: [0.0, 0.0, 1.0], dist: 1.0 }];
        let mv = [VertexMove { from: [1.0, 1.0, 1.0], to: [1.0, 1.0, 1.5], ex: 1 }];
        let (moved, _) = apply_edits(&mut ms, &[0], &mv, &ex);
        assert!(moved >= 2, "dessus de la face et haut des faces latérales : {moved}");
        let pts: Vec<V3> = tris(&ms[0]).into_iter().flatten().collect();
        let near = |p: V3| pts.iter().any(|q| (*q - p).len() < 1e-5);
        assert!(near(v3(1.0, 1.0, 2.5)), "le coin du dessus a monté");
        assert!(!near(v3(1.0, 1.0, 2.0)), "plus de coin du dessus à l'ancienne place");
        assert!(near(v3(1.0, 1.0, 1.0)), "le coin du bas n'a pas bougé");
        assert!(closed(&ms[0]), "le volume reste fermé");
        // le même déplacement en couche 0 bouge le bas (et le dessus qui en est copié)
        let mut ms0 = vec![cube()];
        let mv0 = [VertexMove { from: [1.0, 1.0, 1.0], to: [1.0, 1.0, 1.5], ex: 0 }];
        apply_edits(&mut ms0, &[0], &mv0, &ex);
        let pts0: Vec<V3> = tris(&ms0[0]).into_iter().flatten().collect();
        assert!(pts0.iter().any(|q| (*q - v3(1.0, 1.0, 2.5)).len() < 1e-5));
        assert!(!pts0.iter().any(|q| (*q - v3(1.0, 1.0, 1.0)).len() < 1e-5));
    }

    #[test]
    fn bord_a_cheval_sur_un_arrondi_pas_de_nageoire() {
        // un carré de deux triangles dont le sommet partagé (1, 1) est écrit deux fois à 0,2 mm près, de part et
        // d'autre d'un arrondi au millimètre : le bord intérieur n'est pas un bord libre
        let v = |x: f32, y: f32| vec![x, y, 0.0, 0.0, 0.0, 1.0, x, y];
        let mut m = GMesh { layout: lay(), verts: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0004), v(0.0, 0.0), v(1.0, 1.0006), v(0.0, 1.0)], idx: vec![0, 1, 2, 3, 4, 5] };
        let added = extrude_tris(&mut m, &[0, 1], v3(0.0, 0.0, 1.0), 0.5);
        assert_eq!(added, 8, "4 bords libres seulement");
    }

    #[test]
    fn faces_laterales_tangente_recalculee() {
        // disposition avec tangente (x, y, z, sens) : la tangente des faces latérales est perpendiculaire à leur
        // normale et suit le sens des UV
        let layout = Layout { int: vec![false; 12], pos: 0, nrm: Some(3), uv: Some(6), tan: Some(8), ..Default::default() };
        let v = |x: f32, y: f32| vec![x, y, 0.0, 0.0, 0.0, 1.0, x, y, 1.0, 0.0, 0.0, 1.0];
        let mut m = GMesh { layout, verts: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)], idx: vec![0, 1, 2, 0, 2, 3] };
        extrude_tris(&mut m, &[0, 1], v3(0.0, 0.0, 1.0), 0.5);
        let side = &m.verts[8..];
        assert_eq!(side.len(), 16);
        for q in side {
            let (n, t) = (v3(q[3], q[4], q[5]), v3(q[8], q[9], q[10]));
            assert!(n.dot(t).abs() < 1e-4, "tangente {t:?} pas perpendiculaire à la normale {n:?}");
            assert!((t.len() - 1.0).abs() < 1e-4);
            assert!(q[11] == 1.0 || q[11] == -1.0);
        }
    }
}
