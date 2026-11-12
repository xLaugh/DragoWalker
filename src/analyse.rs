//! Analyse d'un bâtiment du jeu avant d'en faire un intérieur : vitres, sols et étages, murs extérieurs.
//! Première étape du plan « bâtiment du jeu → intérieur (MLO) ».

use crate::gta::drawable::RModel;
use crate::math::{v3, Quat, V3};

/// Genre d'un triangle analysé.
pub const K_GLASS: u8 = 0;
pub const K_FLOOR: u8 = 1;
pub const K_WALL_OUT: u8 = 2;
pub const K_WALL_IN: u8 = 3;
pub const K_CEIL: u8 = 4;
pub const K_OTHER: u8 = 5;
/// Surface tournée vers le haut que rien ne couvre : toit, terrasse, trottoir — pas un sol d'étage.
pub const K_ROOF: u8 = 6;

/// Un sol compte comme sol d'étage seulement si un rayon vertical, parti de lui, touche une autre surface du
/// bâtiment au-dessus (plafond, toit) plus haut que ce seuil (m) — **estimé**.
const COVER_MIN: f32 = 0.05;

/// Un sol plat orienté vers le haut compte comme sol au-delà de ce cosinus (≈ 32°) — estimé.
const FLOOR_COS: f32 = 0.85;
/// Un mur est presque vertical : |normale.z| sous ce seuil — estimé.
const WALL_COS: f32 = 0.25;
/// Hauteurs de sol regroupées par tranches de 0,5 m — estimé.
const LEVEL_BIN: f32 = 0.5;

pub struct Analysis {
    pub id: u64,
    pub name: String,
    /// triangles dans le monde et leur genre
    pub tris: Vec<([V3; 3], u8)>,
    pub tri_count: usize,
    pub glass_area: f32,
    pub wall_out_area: f32,
    pub wall_in_area: f32,
    /// étages : hauteur (monde) et surface de sol
    pub levels: Vec<(f32, f32)>,
    /// taille de l'emprise (m)
    pub size: V3,
    pub roof_z: f32,
    pub show: bool,
    /// placement du bâtiment (pour revenir dans son repère)
    pub pos: V3,
    pub rot: Quat,
    /// direction des murs dans le repère du modèle (radians, entre -45° et 45°) : beaucoup de modèles du jeu ont
    /// leurs murs tournés par rapport à leurs propres axes ; les pièces suivent les murs
    pub wall_yaw: f32,
}

impl Analysis {
    /// Repère des pièces : celui du bâtiment, tourné de la direction des murs.
    pub fn room_rot(&self) -> Quat {
        self.rot.mul(Quat::from_z(self.wall_yaw)).normalized()
    }
}

/// Rayon contre triangle (Möller–Trumbore) : distance le long du rayon, si touché.
fn ray_tri(o: V3, d: V3, t: &[V3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-7 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) * inv)
}

fn area(a: V3, b: V3, c: V3) -> f32 {
    (b - a).cross(c - a).len() * 0.5
}

/// Analyse un modèle placé en `pos` / `rot` (orientation) / échelles.
pub fn analyse(id: u64, name: &str, m: &RModel, pos: V3, rot: Quat, sxy: f32, sz: f32) -> Analysis {
    let p = |i: u32| {
        let v = m.verts[i as usize].pos;
        v3(v[0] * sxy, v[1] * sxy, v[2] * sz)
    };
    // emprise (repère du modèle)
    let mut mn = v3(f32::MAX, f32::MAX, f32::MAX);
    let mut mx = v3(f32::MIN, f32::MIN, f32::MIN);
    for v in &m.verts {
        let q = v3(v.pos[0] * sxy, v.pos[1] * sxy, v.pos[2] * sz);
        mn = mn.min(q);
        mx = mx.max(q);
    }
    // 1. triangles du modèle (repère local) et leur genre provisoire
    let mut local: Vec<([V3; 3], V3, u8, f32)> = Vec::new();
    for g in &m.geoms {
        // vitres : matériaux de verre, comme les GlassBatches de CodeWalker (shader glass*.sps)
        let is_glass = g.glass;
        let end = (g.first + g.count).min(m.idx.len() as u32);
        let mut i = g.first;
        while i + 2 < end {
            let (ia, ib, ic) = (m.idx[i as usize], m.idx[i as usize + 1], m.idx[i as usize + 2]);
            let (a, b, c) = (p(ia), p(ib), p(ic));
            i += 3;
            let ar = area(a, b, c);
            if ar < 1e-4 {
                continue;
            }
            // sens de la face : celui des normales du modèle (l'ordre des sommets n'est pas fiable)
            let mut n = (b - a).cross(c - a).norm();
            let vn = [ia, ib, ic].iter().fold(V3::ZERO, |s, &k| {
                let q = m.verts[k as usize].nrm;
                s + v3(q[0], q[1], q[2])
            });
            if n.dot(vn) < 0.0 {
                n = n * -1.0;
            }
            let kind = if is_glass {
                K_GLASS
            } else if n.z > FLOOR_COS {
                K_FLOOR
            } else if n.z < -FLOOR_COS {
                K_CEIL
            } else if n.z.abs() < WALL_COS {
                K_WALL_OUT
            } else {
                K_OTHER
            };
            local.push(([a, b, c], n, kind, ar));
        }
    }
    // 2. grille 2D des triangles (cases de 4 m) pour lancer des rayons vite
    const CELL: f32 = 4.0;
    let cell = |x: f32, y: f32| (((x - mn.x) / CELL).floor() as i32, ((y - mn.y) / CELL).floor() as i32);
    let mut grid: std::collections::HashMap<(i32, i32), Vec<u32>> = std::collections::HashMap::new();
    for (ti, (t, _, _, _)) in local.iter().enumerate() {
        let (x0, y0) = cell(t[0].x.min(t[1].x).min(t[2].x), t[0].y.min(t[1].y).min(t[2].y));
        let (x1, y1) = cell(t[0].x.max(t[1].x).max(t[2].x), t[0].y.max(t[1].y).max(t[2].y));
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                grid.entry((cx, cy)).or_default().push(ti as u32);
            }
        }
    }
    // 3. un mur est extérieur si le rayon qui part de lui vers l'extérieur (sa normale) ne touche rien d'autre
    //    du bâtiment — sinon il est derrière une autre surface (intérieur, renfoncement)
    let mut stamp = vec![0u32; local.len()];
    let reach = (mx - mn).len() + 1.0;
    for wi in 0..local.len() {
        if local[wi].2 != K_WALL_OUT {
            continue;
        }
        let (t, n, _, _) = local[wi];
        let o = (t[0] + t[1] + t[2]) * (1.0 / 3.0) + n * 0.05;
        let mut blocked = false;
        let mut d = 0.0f32;
        let mut last_cell = (i32::MIN, i32::MIN);
        while d <= reach && !blocked {
            let q = o + n * d;
            let c = cell(q.x, q.y);
            if c != last_cell {
                last_cell = c;
                if let Some(list) = grid.get(&c) {
                    for &ti in list {
                        let ti = ti as usize;
                        if ti == wi || stamp[ti] == wi as u32 + 1 {
                            continue;
                        }
                        stamp[ti] = wi as u32 + 1;
                        if ray_tri(o, n, &local[ti].0).map(|h| h > 0.02).unwrap_or(false) {
                            blocked = true;
                            break;
                        }
                    }
                }
            }
            d += CELL * 0.5;
        }
        if blocked {
            local[wi].2 = K_WALL_IN;
        }
    }
    // 3 bis. sol couvert ou non : un toit (ou une terrasse) n'a rien au-dessus de lui ; il n'est pas un étage
    let up = v3(0.0, 0.0, 1.0);
    for fi in 0..local.len() {
        if local[fi].2 != K_FLOOR {
            continue;
        }
        let t = local[fi].0;
        let o = (t[0] + t[1] + t[2]) * (1.0 / 3.0) + up * 0.02;
        let covered = grid.get(&cell(o.x, o.y)).map(|list| list.iter().any(|&ti| ti as usize != fi && ray_tri(o, up, &local[ti as usize].0).map(|h| h > COVER_MIN).unwrap_or(false))).unwrap_or(false);
        if !covered {
            local[fi].2 = K_ROOF;
        }
    }
    // 3 ter. direction des murs : moyenne (pondérée par la surface) des directions de leurs normales, modulo 90° —
    //        moyenne circulaire de 4θ, comme pour des directions qui se répètent tous les quarts de tour
    let (mut c4, mut s4) = (0.0f32, 0.0f32);
    for (_, n, k, ar) in &local {
        if (*k == K_WALL_OUT || *k == K_WALL_IN || *k == K_GLASS) && n.z.abs() < WALL_COS {
            let a = n.y.atan2(n.x) * 4.0;
            c4 += ar * a.cos();
            s4 += ar * a.sin();
        }
    }
    let wall_yaw = if c4.abs() + s4.abs() > 1e-3 { s4.atan2(c4) / 4.0 } else { 0.0 };
    // 4. totaux et passage dans le monde
    let mut tris = Vec::with_capacity(local.len());
    let (mut glass, mut wout, mut win) = (0.0f32, 0.0f32, 0.0f32);
    let mut bins: std::collections::BTreeMap<i32, f32> = std::collections::BTreeMap::new();
    for (t, _, kind, ar) in &local {
        match *kind {
            K_GLASS => glass += ar,
            K_WALL_OUT => wout += ar,
            K_WALL_IN => win += ar,
            K_FLOOR => {
                let z = (t[0].z + t[1].z + t[2].z) / 3.0;
                *bins.entry((z / LEVEL_BIN).round() as i32).or_insert(0.0) += ar;
            }
            _ => {}
        }
        let w = |q: V3| pos + rot.rotate(q);
        tris.push(([w(t[0]), w(t[1]), w(t[2])], *kind));
    }
    // étages : tranches voisines regroupées, garder les grandes surfaces
    let total: f32 = bins.values().sum();
    let mut levels: Vec<(f32, f32)> = Vec::new();
    let mut cur: Option<(f32, f32, i32)> = None; // (z pondéré, surface, dernière tranche)
    for (&b, &a) in &bins {
        match cur {
            Some((zs, s, last)) if b - last <= 1 => cur = Some((zs + b as f32 * LEVEL_BIN * a, s + a, b)),
            _ => {
                if let Some((zs, s, _)) = cur {
                    levels.push((zs / s, s));
                }
                cur = Some((b as f32 * LEVEL_BIN * a, a, b));
            }
        }
    }
    if let Some((zs, s, _)) = cur {
        levels.push((zs / s, s));
    }
    // un étage doit avoir au moins 8 m² ou 3 % de toute la surface de sol — estimé
    levels.retain(|(_, s)| *s >= 8.0_f32.max(total * 0.03));
    // coque sans plancher (la plupart des bâtiments du jeu) : un étage au pied du modèle, sous le toit
    let ground_only = levels.is_empty() && local.iter().any(|(_, _, k, _)| *k == K_ROOF);
    if ground_only {
        levels.push((mn.z, (mx.x - mn.x) * (mx.y - mn.y)));
    }
    let levels = levels.into_iter().map(|(z, s)| (pos.z + z, s)).collect();
    Analysis {
        id,
        name: name.to_string(),
        tri_count: tris.len(),
        tris,
        glass_area: glass,
        wall_out_area: wout,
        wall_in_area: win,
        levels,
        size: mx - mn,
        roof_z: pos.z + mx.z,
        show: true,
        pos,
        rot,
        wall_yaw,
    }
}

pub fn kind_color(k: u8) -> Option<[u8; 4]> {
    match k {
        K_GLASS => Some([70, 170, 255, 130]),
        K_FLOOR => Some([70, 220, 90, 110]),
        K_WALL_OUT => Some([255, 150, 40, 80]),
        K_WALL_IN => Some([200, 110, 255, 80]),
        K_ROOF => Some([150, 150, 150, 50]),
        _ => None,
    }
}

/// Pièces proposées pour un étage, dans le repère du bâtiment.
pub struct RoomPlan {
    /// (nom, coin bas, coin haut)
    pub rooms: Vec<(String, V3, V3)>,
    /// portails (pièce de départ, pièce d'arrivée, quatre coins) ; pièce 0 = extérieur
    pub portals: Vec<(u32, u32, [V3; 4])>,
    pub floor_area: f32,
}

/// Case de la grille de l'étage (m) — estimé.
const ROOM_CELL: f32 = 0.5;
/// Hauteur où l'on coupe les murs pour trouver les cloisons (m au-dessus du sol) — estimé.
const WALL_CUT: f32 = 1.2;
/// Une pièce trouvée doit faire au moins 4 m² — estimé.
const MIN_ROOM_M2: f32 = 4.0;
/// Zone écartée (porche, avant-toit) quand plus de cette part de son bord donne directement dehors — estimé.
const PORCH_OPEN: f32 = 0.35;
/// Côté minimum d'une pièce (m) — estimé.
const MIN_ROOM_SIDE: f32 = 1.5;
/// Rectangles au plus par zone fermée — estimé.
const MAX_RECTS_PER_ZONE: usize = 12;
/// Distance au-delà d'un côté de pièce où l'on cherche l'extérieur (épaisseur de mur, m) — estimé.
const EXIT_PROBE: f32 = 1.5;
/// Hauteur de pièce par défaut quand il n'y a pas d'étage au-dessus, et limites — estimé.
const DEFAULT_ROOM_H: f32 = 3.2;

fn inside_tri_2d(px: f32, py: f32, t: &[V3; 3]) -> bool {
    let s = |a: V3, b: V3| (b.x - a.x) * (py - a.y) - (b.y - a.y) * (px - a.x);
    let (d1, d2, d3) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Hauteur d'un point (x, y) sur le plan d'un triangle.
fn z_on_tri(px: f32, py: f32, t: &[V3; 3]) -> f32 {
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    if n.z.abs() < 1e-6 {
        return t[0].z.max(t[1].z).max(t[2].z);
    }
    t[0].z - (n.x * (px - t[0].x) + n.y * (py - t[0].y)) / n.z
}

/// Propose les pièces de l'étage `li` en suivant la forme du bâtiment analysé :
/// - grille de 0,5 m ; une case compte si elle est sous le bâtiment (une surface au-dessus d'elle, plus haut que
///   0,3 m : plafond, toit) et, s'il y a un vrai sol à cet étage, sur ce sol (pas les toits, terrasses, trottoirs) ;
///   coque sans plancher : toute l'emprise couverte, au pied du modèle ;
/// - murs, vitres et autres parois coupés à 1,2 m au-dessus du sol : ils séparent les zones fermées ;
/// - chaque zone est découpée en rectangles qui y tiennent entièrement (le plus grand d'abord, au moins 4 m² et
///   1,5 m de côté) : une pièce d'intérieur est une boîte, la zone en L devient deux pièces, sans déborder ;
///   deux rectangles voisins d'une même zone sont reliés par un passage ouvert sur tout leur côté commun ;
/// - hauteur des pièces : sous le plafond trouvé au-dessus (médiane), ou l'étage suivant ;
/// - entrée : au milieu du plus long côté d'une pièce qui donne sur l'extérieur.
pub fn propose_rooms(a: &Analysis, li: usize) -> Option<RoomPlan> {
    let (zw, _) = *a.levels.get(li)?;
    // repère des pièces : celui du bâtiment tourné de la direction des murs (même origine, même hauteur)
    let inv = a.room_rot().inverse();
    let loc = |w: V3| inv.rotate(w - a.pos);
    let z0 = zw - a.pos.z;
    let next = a.levels.get(li + 1).map(|l| l.0 - a.pos.z);
    let tris: Vec<([V3; 3], u8)> = a.tris.iter().map(|(t, k)| ([loc(t[0]), loc(t[1]), loc(t[2])], *k)).collect();
    // sol de l'étage (sols couverts seulement : les toits sont K_ROOF)
    let floors: Vec<&[V3; 3]> = tris.iter().filter(|(t, k)| *k == K_FLOOR && ((t[0].z + t[1].z + t[2].z) / 3.0 - z0).abs() < 0.6).map(|(t, _)| t).collect();
    // surfaces au-dessus de l'étage : ce qui couvre une case (plafond, toit, étage du dessus)
    let over: Vec<&[V3; 3]> = tris.iter().filter(|(t, _)| t[0].z.max(t[1].z).max(t[2].z) > z0 + 0.3).map(|(t, _)| t).collect();
    // étendue de la grille : le sol de l'étage, et toute l'emprise couverte au rez-de-chaussée ou sans sol
    let src: Vec<&[V3; 3]> = if floors.is_empty() || li == 0 { floors.iter().chain(over.iter()).copied().collect() } else { floors.clone() };
    if src.is_empty() {
        return None;
    }
    let mut mn = v3(f32::MAX, f32::MAX, 0.0);
    let mut mx = v3(f32::MIN, f32::MIN, 0.0);
    for t in src.iter() {
        for p in t.iter() {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    let nx = (((mx.x - mn.x) / ROOM_CELL).ceil() as usize).clamp(1, 1200);
    let ny = (((mx.y - mn.y) / ROOM_CELL).ceil() as usize).clamp(1, 1200);
    let idx = |x: usize, y: usize| y * nx + x;
    let ctr = |x: usize, y: usize| (mn.x + (x as f32 + 0.5) * ROOM_CELL, mn.y + (y as f32 + 0.5) * ROOM_CELL);
    // ce qui couvre chaque case : hauteur de la surface la plus basse au-dessus de z0 + 0,3 m
    let mut ceil = vec![f32::MAX; nx * ny];
    for t in &over {
        let (x0, x1) = (((t[0].x.min(t[1].x).min(t[2].x) - mn.x) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].x.max(t[1].x).max(t[2].x) - mn.x) / ROOM_CELL).ceil().max(0.0) as usize).min(nx));
        let (y0, y1) = (((t[0].y.min(t[1].y).min(t[2].y) - mn.y) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].y.max(t[1].y).max(t[2].y) - mn.y) / ROOM_CELL).ceil().max(0.0) as usize).min(ny));
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = ctr(x, y);
                if inside_tri_2d(px, py, t) {
                    let z = z_on_tri(px, py, t);
                    if z > z0 + 0.3 && z < ceil[idx(x, y)] {
                        ceil[idx(x, y)] = z;
                    }
                }
            }
        }
    }
    let mut floor = vec![false; nx * ny];
    if floors.is_empty() {
        for i in 0..nx * ny {
            floor[i] = ceil[i] < f32::MAX;
        }
    } else {
        for t in &floors {
            let (x0, x1) = (((t[0].x.min(t[1].x).min(t[2].x) - mn.x) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].x.max(t[1].x).max(t[2].x) - mn.x) / ROOM_CELL).ceil().max(0.0) as usize).min(nx));
            let (y0, y1) = (((t[0].y.min(t[1].y).min(t[2].y) - mn.y) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].y.max(t[1].y).max(t[2].y) - mn.y) / ROOM_CELL).ceil().max(0.0) as usize).min(ny));
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = ctr(x, y);
                    if inside_tri_2d(px, py, t) && ceil[idx(x, y)] < f32::MAX {
                        floor[idx(x, y)] = true;
                    }
                }
            }
        }
        // rez-de-chaussée : les bâtiments du jeu sont des coques sans plancher intérieur (seuls le porche, le
        // perron… ont un sol) : toute l'emprise couverte compte
        if li == 0 {
            for i in 0..nx * ny {
                floor[i] = floor[i] || ceil[i] < f32::MAX;
            }
        }
    }
    // hauteur des pièces : plafond médian au-dessus du sol, sans dépasser l'étage suivant
    let mut hs: Vec<f32> = (0..nx * ny).filter(|&i| floor[i]).map(|i| ceil[i] - z0).collect();
    hs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut h = hs.get(hs.len() / 2).copied().unwrap_or(DEFAULT_ROOM_H) - 0.05;
    if let Some(z1) = next {
        h = h.min(z1 - z0 - 0.3);
    }
    let h = h.clamp(2.4, 8.0);
    // murs, vitres et autres parois coupés à 1,2 m au-dessus du sol
    let zc = z0 + WALL_CUT;
    let mut wall = vec![false; nx * ny];
    for (t, k) in &tris {
        if *k == K_FLOOR || *k == K_CEIL {
            continue;
        }
        let (lo, hi) = (t[0].z.min(t[1].z).min(t[2].z), t[0].z.max(t[1].z).max(t[2].z));
        if !(lo <= zc && hi >= zc) || hi - lo < 1e-3 {
            continue;
        }
        let mut pts = Vec::new();
        for (p, q) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if (p.z - zc) * (q.z - zc) <= 0.0 && (p.z - q.z).abs() > 1e-6 {
                let f = (zc - p.z) / (q.z - p.z);
                pts.push(p + (q - p) * f);
            }
        }
        if pts.len() < 2 {
            continue;
        }
        let (p, q) = (pts[0], pts[1]);
        let n = (((q - p).len() / (ROOM_CELL * 0.5)).ceil() as usize).max(1);
        for s in 0..=n {
            let r = p + (q - p) * (s as f32 / n as f32);
            let (cx, cy) = ((r.x - mn.x) / ROOM_CELL, (r.y - mn.y) / ROOM_CELL);
            if cx >= 0.0 && cy >= 0.0 && (cx as usize) < nx && (cy as usize) < ny {
                wall[idx(cx as usize, cy as usize)] = true;
            }
        }
    }
    // zones fermées (voisins directs)
    let free: Vec<bool> = (0..nx * ny).map(|i| floor[i] && !wall[i]).collect();
    let mut comp = vec![u32::MAX; nx * ny];
    let mut zones: Vec<Vec<usize>> = Vec::new();
    for start in 0..nx * ny {
        if !free[start] || comp[start] != u32::MAX {
            continue;
        }
        let id = zones.len() as u32;
        let mut stack = vec![start];
        comp[start] = id;
        let mut cells = Vec::new();
        while let Some(c) = stack.pop() {
            cells.push(c);
            let (x, y) = (c % nx, c / nx);
            let mut push = |nx_: usize, ny_: usize| {
                let j = idx(nx_, ny_);
                if free[j] && comp[j] == u32::MAX {
                    comp[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 { push(x - 1, y); }
            if x + 1 < nx { push(x + 1, y); }
            if y > 0 { push(x, y - 1); }
            if y + 1 < ny { push(x, y + 1); }
        }
        zones.push(cells);
    }
    let cell_m2 = ROOM_CELL * ROOM_CELL;
    let mut rooms: Vec<(String, V3, V3)> = Vec::new();
    // rectangle (en cases) de chaque pièce : x0, y0, x1, y1 (bornes hautes exclues)
    let mut rects: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
    let mut floor_area = 0.0;
    let door_w = 1.2f32;
    let door_h = h.min(2.4);
    let min_side = (MIN_ROOM_SIDE / ROOM_CELL).ceil() as usize;
    let at = |x: usize, y: usize| v3(mn.x + x as f32 * ROOM_CELL, mn.y + y as f32 * ROOM_CELL, 0.0);
    for (zi, cells) in zones.into_iter().enumerate() {
        let area = cells.len() as f32 * cell_m2;
        if area < MIN_ROOM_M2 {
            continue;
        }
        // zone ouverte sur l'extérieur (porche, avant-toit, auvent) : une grande part de son bord donne sur une
        // case que rien ne couvre, au lieu d'un mur
        let (mut edge, mut open) = (0usize, 0usize);
        for &c in &cells {
            let (x, y) = ((c % nx) as isize, (c / nx) as isize);
            for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                let (qx, qy) = (x + dx, y + dy);
                let inside = qx >= 0 && qy >= 0 && (qx as usize) < nx && (qy as usize) < ny;
                if inside && comp[idx(qx as usize, qy as usize)] == zi as u32 {
                    continue;
                }
                edge += 1;
                if !inside || ceil[idx(qx as usize, qy as usize)] == f32::MAX {
                    open += 1;
                }
            }
        }
        if edge > 0 && open as f32 > edge as f32 * PORCH_OPEN {
            continue;
        }
        floor_area += area;
        // rectangles qui tiennent dans la zone, le plus grand d'abord
        let mut left = vec![false; nx * ny];
        for &c in &cells {
            left[c] = true;
        }
        let first = rects.len();
        for _ in 0..MAX_RECTS_PER_ZONE {
            let Some((x0, y0, x1, y1)) = largest_rect(&left, nx, ny) else { break };
            if ((x1 - x0) * (y1 - y0)) as f32 * cell_m2 < MIN_ROOM_M2 || (x1 - x0) < min_side || (y1 - y0) < min_side {
                break;
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    left[idx(x, y)] = false;
                }
            }
            let n = rooms.len() + 1;
            let (lo, hi) = (at(x0, y0), at(x1, y1));
            rooms.push((format!("piece_{n}"), v3(lo.x, lo.y, z0), v3(hi.x, hi.y, z0 + h)));
            rects.push((x0, y0, x1, y1));
        }
        // rectangles voisins d'une même zone : passage ouvert sur tout le côté commun
        for i in first..rects.len() {
            for j in (i + 1)..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let (ri, rj) = ((i + 1) as u32, (j + 1) as u32);
                // côté vertical commun (x)
                for (xa, xb) in [(a.2, b.0), (b.2, a.0)] {
                    if xa == xb {
                        let (y0, y1) = (a.1.max(b.1), a.3.min(b.3));
                        if y1 > y0 && (y1 - y0) as f32 * ROOM_CELL >= door_w {
                            let x = at(xa, 0).x;
                            let (ya, yb) = (at(0, y0).y, at(0, y1).y);
                            portals.push((ri, rj, [v3(x, ya, z0), v3(x, ya, z0 + h), v3(x, yb, z0 + h), v3(x, yb, z0)]));
                        }
                    }
                }
                // côté horizontal commun (y)
                for (ya, yb) in [(a.3, b.1), (b.3, a.1)] {
                    if ya == yb {
                        let (x0, x1) = (a.0.max(b.0), a.2.min(b.2));
                        if x1 > x0 && (x1 - x0) as f32 * ROOM_CELL >= door_w {
                            let y = at(0, ya).y;
                            let (xa, xb) = (at(x0, 0).x, at(x1, 0).x);
                            portals.push((ri, rj, [v3(xa, y, z0), v3(xa, y, z0 + h), v3(xb, y, z0 + h), v3(xb, y, z0)]));
                        }
                    }
                }
            }
        }
    }
    if rooms.is_empty() {
        return None;
    }
    // entrée : le plus long côté d'une pièce qui donne sur l'extérieur (au-delà du mur : plus rien sous le
    // bâtiment, ou hors de la grille), au milieu de ce côté
    let outside = |x: isize, y: isize| x < 0 || y < 0 || x >= nx as isize || y >= ny as isize || ceil[idx(x as usize, y as usize)] == f32::MAX;
    let reach = (EXIT_PROBE / ROOM_CELL).ceil() as isize;
    let mut best: Option<(f32, usize, [V3; 4])> = None;
    for (ri, &(x0, y0, x1, y1)) in rects.iter().enumerate() {
        // (cases le long du côté, direction vers l'extérieur, coordonnée du côté)
        let sides: [(Vec<(isize, isize)>, (isize, isize), bool, usize); 4] = [
            ((x0..x1).map(|x| (x as isize, y0 as isize)).collect(), (0, -1), false, y0),
            ((x0..x1).map(|x| (x as isize, y1 as isize - 1)).collect(), (0, 1), false, y1),
            ((y0..y1).map(|y| (x0 as isize, y as isize)).collect(), (-1, 0), true, x0),
            ((y0..y1).map(|y| (x1 as isize - 1, y as isize)).collect(), (1, 0), true, x1),
        ];
        for (cells, (dx, dy), vertical, coord) in sides.iter() {
            let ext = cells.iter().filter(|(x, y)| (1..=reach).any(|k| outside(x + dx * k, y + dy * k))).count();
            if (ext as f32) < cells.len() as f32 * 0.6 {
                continue;
            }
            let len = cells.len() as f32 * ROOM_CELL;
            if len < door_w || best.as_ref().map(|b| len <= b.0).unwrap_or(false) {
                continue;
            }
            let quad = if *vertical {
                let x = at(*coord, 0).x;
                let cy = (at(0, y0).y + at(0, y1).y) * 0.5;
                [v3(x, cy - door_w * 0.5, z0), v3(x, cy - door_w * 0.5, z0 + door_h), v3(x, cy + door_w * 0.5, z0 + door_h), v3(x, cy + door_w * 0.5, z0)]
            } else {
                let y = at(0, *coord).y;
                let cx = (at(x0, 0).x + at(x1, 0).x) * 0.5;
                [v3(cx - door_w * 0.5, y, z0), v3(cx - door_w * 0.5, y, z0 + door_h), v3(cx + door_w * 0.5, y, z0 + door_h), v3(cx + door_w * 0.5, y, z0)]
            };
            best = Some((len, ri, quad));
        }
    }
    // aucun côté dehors (pièce au cœur du bâtiment) : côté sud de la pièce la plus au sud
    let (ei, eq) = match best {
        Some((_, ri, q)) => (ri, q),
        None => {
            let (si, s) = rooms.iter().enumerate().min_by(|a, b| a.1 .1.y.partial_cmp(&b.1 .1.y).unwrap_or(std::cmp::Ordering::Equal)).map(|(i, r)| (i, r.clone())).unwrap();
            let cx = (s.1.x + s.2.x) * 0.5;
            (si, [v3(cx - door_w * 0.5, s.1.y, z0), v3(cx - door_w * 0.5, s.1.y, z0 + door_h), v3(cx + door_w * 0.5, s.1.y, z0 + door_h), v3(cx + door_w * 0.5, s.1.y, z0)])
        }
    };
    portals.push(((ei + 1) as u32, 0, eq));
    Some(RoomPlan { rooms, portals, floor_area })
}

/// Plus grand rectangle de cases à vrai (méthode des histogrammes) : (x0, y0, x1, y1), bornes hautes exclues.
fn largest_rect(m: &[bool], nx: usize, ny: usize) -> Option<(usize, usize, usize, usize)> {
    let mut hgt = vec![0usize; nx];
    let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
    for y in 0..ny {
        for x in 0..nx {
            hgt[x] = if m[y * nx + x] { hgt[x] + 1 } else { 0 };
        }
        // pile des colonnes de hauteur croissante
        let mut st: Vec<usize> = Vec::new();
        for x in 0..=nx {
            let cur = if x < nx { hgt[x] } else { 0 };
            while let Some(&top) = st.last() {
                if hgt[top] <= cur {
                    break;
                }
                st.pop();
                let hh = hgt[top];
                let left = st.last().map(|&l| l + 1).unwrap_or(0);
                let area = hh * (x - left);
                if area > 0 && best.map(|b| area > b.0).unwrap_or(true) {
                    best = Some((area, (left, y + 1 - hh, x, y + 1)));
                }
            }
            st.push(x);
        }
    }
    best.map(|b| b.1)
}
