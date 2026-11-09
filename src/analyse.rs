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
    }
}

pub fn kind_color(k: u8) -> Option<[u8; 4]> {
    match k {
        K_GLASS => Some([70, 170, 255, 130]),
        K_FLOOR => Some([70, 220, 90, 110]),
        K_WALL_OUT => Some([255, 150, 40, 80]),
        K_WALL_IN => Some([200, 110, 255, 80]),
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
/// Une zone ouverte plus grande est découpée en pièces d'environ 7 m de côté — estimé.
const SPLIT_ABOVE_M2: f32 = 120.0;
const SPLIT_SIZE: f32 = 7.0;
/// Hauteur de pièce par défaut quand il n'y a pas d'étage au-dessus, et limites — estimé.
const DEFAULT_ROOM_H: f32 = 3.2;

fn inside_tri_2d(px: f32, py: f32, t: &[V3; 3]) -> bool {
    let s = |a: V3, b: V3| (b.x - a.x) * (py - a.y) - (b.y - a.y) * (px - a.x);
    let (d1, d2, d3) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Propose les pièces de l'étage `li` : sol de l'étage sur une grille, murs coupés à 1,2 m, zones fermées ;
/// une grande zone ouverte (coque vide du jeu) est découpée en pièces d'environ 7 m reliées par des passages.
pub fn propose_rooms(a: &Analysis, li: usize) -> Option<RoomPlan> {
    let (zw, _) = *a.levels.get(li)?;
    let inv = a.rot.inverse();
    let loc = |w: V3| inv.rotate(w - a.pos);
    let z0 = zw - a.pos.z;
    let z1 = a.levels.get(li + 1).map(|l| l.0 - a.pos.z).unwrap_or(z0 + DEFAULT_ROOM_H);
    let h = (z1 - z0 - 0.3).clamp(2.4, 8.0);
    let tris: Vec<([V3; 3], u8)> = a.tris.iter().map(|(t, k)| ([loc(t[0]), loc(t[1]), loc(t[2])], *k)).collect();
    // sol de l'étage
    let floors: Vec<&[V3; 3]> = tris.iter().filter(|(t, k)| *k == K_FLOOR && ((t[0].z + t[1].z + t[2].z) / 3.0 - z0).abs() < 0.6).map(|(t, _)| t).collect();
    if floors.is_empty() {
        return None;
    }
    let mut mn = v3(f32::MAX, f32::MAX, 0.0);
    let mut mx = v3(f32::MIN, f32::MIN, 0.0);
    for t in &floors {
        for p in t.iter() {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    let nx = (((mx.x - mn.x) / ROOM_CELL).ceil() as usize).clamp(1, 1200);
    let ny = (((mx.y - mn.y) / ROOM_CELL).ceil() as usize).clamp(1, 1200);
    let idx = |x: usize, y: usize| y * nx + x;
    let ctr = |x: usize, y: usize| (mn.x + (x as f32 + 0.5) * ROOM_CELL, mn.y + (y as f32 + 0.5) * ROOM_CELL);
    let mut floor = vec![false; nx * ny];
    for t in &floors {
        let (x0, x1) = (((t[0].x.min(t[1].x).min(t[2].x) - mn.x) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].x.max(t[1].x).max(t[2].x) - mn.x) / ROOM_CELL).ceil() as usize).min(nx));
        let (y0, y1) = (((t[0].y.min(t[1].y).min(t[2].y) - mn.y) / ROOM_CELL).floor().max(0.0) as usize, (((t[0].y.max(t[1].y).max(t[2].y) - mn.y) / ROOM_CELL).ceil() as usize).min(ny));
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = ctr(x, y);
                if inside_tri_2d(px, py, t) {
                    floor[idx(x, y)] = true;
                }
            }
        }
    }
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
    let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
    let mut floor_area = 0.0;
    let door_w = 1.2f32;
    let door_h = h.min(2.4);
    for cells in zones {
        let area = cells.len() as f32 * cell_m2;
        if area < MIN_ROOM_M2 {
            continue;
        }
        floor_area += area;
        let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for &c in &cells {
            let (x, y) = (c % nx, c / nx);
            bx0 = bx0.min(x);
            by0 = by0.min(y);
            bx1 = bx1.max(x + 1);
            by1 = by1.max(y + 1);
        }
        let cset: std::collections::HashSet<usize> = cells.iter().copied().collect();
        if area <= SPLIT_ABOVE_M2 {
            let n = rooms.len() + 1;
            rooms.push((format!("piece_{n}"), v3(mn.x + bx0 as f32 * ROOM_CELL, mn.y + by0 as f32 * ROOM_CELL, z0), v3(mn.x + bx1 as f32 * ROOM_CELL, mn.y + by1 as f32 * ROOM_CELL, z0 + h)));
            continue;
        }
        // grande zone ouverte : grille de pièces d'environ 7 m, gardées si elles couvrent assez de sol
        let step = (SPLIT_SIZE / ROOM_CELL).round() as usize;
        let cols = ((bx1 - bx0) + step - 1) / step;
        let rows = ((by1 - by0) + step - 1) / step;
        let mut grid_room: Vec<Option<u32>> = vec![None; cols * rows];
        for r in 0..rows {
            for c in 0..cols {
                let (gx0, gy0) = (bx0 + c * step, by0 + r * step);
                let (gx1, gy1) = ((gx0 + step).min(bx1), (gy0 + step).min(by1));
                let mut n = 0usize;
                let (mut fx0, mut fy0, mut fx1, mut fy1) = (usize::MAX, usize::MAX, 0usize, 0usize);
                for y in gy0..gy1 {
                    for x in gx0..gx1 {
                        if cset.contains(&idx(x, y)) {
                            n += 1;
                            fx0 = fx0.min(x);
                            fy0 = fy0.min(y);
                            fx1 = fx1.max(x + 1);
                            fy1 = fy1.max(y + 1);
                        }
                    }
                }
                if (n as f32) * cell_m2 < MIN_ROOM_M2 {
                    continue;
                }
                let k = rooms.len() + 1;
                rooms.push((format!("piece_{k}"), v3(mn.x + fx0 as f32 * ROOM_CELL, mn.y + fy0 as f32 * ROOM_CELL, z0), v3(mn.x + fx1 as f32 * ROOM_CELL, mn.y + fy1 as f32 * ROOM_CELL, z0 + h)));
                grid_room[r * cols + c] = Some(k as u32);
            }
        }
        // passages au milieu des côtés communs
        for r in 0..rows {
            for c in 0..cols {
                let Some(a_id) = grid_room[r * cols + c] else { continue };
                let ra = rooms[a_id as usize - 1].clone();
                if c + 1 < cols {
                    if let Some(b_id) = grid_room[r * cols + c + 1] {
                        let rb = &rooms[b_id as usize - 1];
                        let x = ra.2.x.min(rb.1.x.max(ra.2.x));
                        let (y0, y1) = (ra.1.y.max(rb.1.y), ra.2.y.min(rb.2.y));
                        if y1 - y0 > door_w {
                            let cy = (y0 + y1) * 0.5;
                            portals.push((a_id, b_id, [v3(x, cy - door_w * 0.5, z0), v3(x, cy - door_w * 0.5, z0 + door_h), v3(x, cy + door_w * 0.5, z0 + door_h), v3(x, cy + door_w * 0.5, z0)]));
                        }
                    }
                }
                if r + 1 < rows {
                    if let Some(b_id) = grid_room[(r + 1) * cols + c] {
                        let rb = &rooms[b_id as usize - 1];
                        let y = ra.2.y.min(rb.1.y.max(ra.2.y));
                        let (x0, x1) = (ra.1.x.max(rb.1.x), ra.2.x.min(rb.2.x));
                        if x1 - x0 > door_w {
                            let cx = (x0 + x1) * 0.5;
                            portals.push((a_id, b_id, [v3(cx - door_w * 0.5, y, z0), v3(cx - door_w * 0.5, y, z0 + door_h), v3(cx + door_w * 0.5, y, z0 + door_h), v3(cx + door_w * 0.5, y, z0)]));
                        }
                    }
                }
            }
        }
    }
    if rooms.is_empty() {
        return None;
    }
    // entrée : de la pièce la plus au sud vers l'extérieur, au milieu de son côté sud
    let (si, s) = rooms.iter().enumerate().min_by(|a, b| a.1 .1.y.partial_cmp(&b.1 .1.y).unwrap_or(std::cmp::Ordering::Equal)).map(|(i, r)| (i, r.clone())).unwrap();
    let cx = (s.1.x + s.2.x) * 0.5;
    portals.push(((si + 1) as u32, 0, [v3(cx - door_w * 0.5, s.1.y, z0), v3(cx - door_w * 0.5, s.1.y, z0 + door_h), v3(cx + door_w * 0.5, s.1.y, z0 + door_h), v3(cx + door_w * 0.5, s.1.y, z0)]));
    Some(RoomPlan { rooms, portals, floor_area })
}
