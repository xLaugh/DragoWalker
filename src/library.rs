//! Bibliothèque de props : une fenêtre avec tous les props du jeu en vignettes 3D, une recherche, un aperçu
//! qui tourne en direct, et la pose d'un clic, d'un double-clic ou en glissant la vignette sur la carte.

use crate::app::{DwApp, Tool};
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke};
use std::collections::HashMap;

/// Une vignette prête à dessiner : triangles déjà projetés dans un carré [-1, 1], du plus loin au plus proche,
/// avec la couleur de la texture du jeu à chaque coin.
pub struct Thumb {
    pub tris: Vec<([Pos2; 3], [Color32; 3])>,
    pub last: u64,
    pub dims: V3,
}

/// Modèle du jeu avec ses coordonnées de texture et ses textures décodées (une par partie).
pub struct TexModel {
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    /// triangles et partie de chacun
    pub tris: Vec<([u32; 3], u16)>,
    /// petite image par partie (pour les vignettes), grande pour l'aperçu
    pub small: Vec<Option<crate::texdecode::Rgba>>,
    pub big: Vec<Option<crate::texdecode::Rgba>>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// triangle vu de l'extérieur (outside_mask) : l'intérieur d'un bâtiment n'est pas dessiné dans les vignettes
    /// et l'aperçu
    pub outside: Vec<bool>,
}

/// Résolution du rendu en profondeur qui repère les triangles vus de l'extérieur — **estimé**.
const OUTSIDE_RES: usize = 160;

/// Triangles vus de l'extérieur : le modèle est rendu en profondeur (sans couleur) depuis 25 directions (12 autour
/// à 9°, 12 autour à 34° de hauteur, 1 de dessus) ; un triangle est gardé s'il est devant (à une marge près) sur au
/// moins un pixel d'une de ces vues, ou, trop petit pour couvrir un pixel, au pixel de son centre. Les murs, sols
/// et meubles de l'intérieur, cachés derrière la façade, sont écartés. Si presque rien ne reste (moins de 5 %),
/// tout est gardé — **estimé** (DragoWalker : les vignettes sont dessinées sans tampon de profondeur).
pub fn outside_mask(pos: &[[f32; 3]], tris: &[([u32; 3], u16)], min: [f32; 3], max: [f32; 3]) -> Vec<bool> {
    let n = tris.len();
    let mut vis = vec![false; n];
    if n == 0 {
        return vis;
    }
    let c = v3((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5);
    let r = (v3(max[0] - min[0], max[1] - min[1], max[2] - min[2]).len() * 0.5).max(0.01);
    let res = OUTSIDE_RES;
    let eps = r * 0.004 + 0.005;
    let mut dirs = Vec::new();
    for (el, off) in [(0.15f32, 0.0f32), (0.6, 0.26)] {
        for i in 0..12 {
            let az = i as f32 * std::f32::consts::TAU / 12.0 + off;
            dirs.push(v3(el.cos() * az.cos(), el.cos() * az.sin(), el.sin()));
        }
    }
    dirs.push(v3(0.0, 0.0, 1.0));
    let mut depth = vec![f32::MAX; res * res];
    let mut p2: Vec<[f32; 3]> = vec![[0.0; 3]; pos.len()];
    for d in dirs {
        let up = if d.z.abs() > 0.9 { v3(0.0, 1.0, 0.0) } else { v3(0.0, 0.0, 1.0) };
        let u = up.cross(d).norm();
        let w = d.cross(u);
        let k = res as f32 / (2.0 * r);
        for (o, p) in p2.iter_mut().zip(pos.iter()) {
            let q = v3(p[0], p[1], p[2]) - c;
            // x, y en pixels ; profondeur : plus petite = plus près de la caméra (placée du côté de d)
            *o = [(q.dot(u) + r) * k, (q.dot(w) + r) * k, -q.dot(d)];
        }
        depth.iter_mut().for_each(|z| *z = f32::MAX);
        // deux passes : la profondeur la plus proche, puis les triangles qui l'atteignent
        for pass in 0..2 {
            for (ti, (t, _)) in tris.iter().enumerate() {
                if pass == 1 && vis[ti] {
                    continue;
                }
                let (Some(a), Some(b), Some(cc)) = (p2.get(t[0] as usize), p2.get(t[1] as usize), p2.get(t[2] as usize)) else { continue };
                let area = (b[0] - a[0]) * (cc[1] - a[1]) - (cc[0] - a[0]) * (b[1] - a[1]);
                if area.abs() < 1e-9 {
                    continue;
                }
                let mut test = |x: usize, y: usize, z: f32| -> bool {
                    let i = y * res + x;
                    if pass == 0 {
                        if z < depth[i] {
                            depth[i] = z;
                        }
                        false
                    } else {
                        z <= depth[i] + eps
                    }
                };
                let x0 = a[0].min(b[0]).min(cc[0]).floor().max(0.0) as usize;
                let x1 = (a[0].max(b[0]).max(cc[0]).ceil().max(0.0) as usize).min(res - 1);
                let y0 = a[1].min(b[1]).min(cc[1]).floor().max(0.0) as usize;
                let y1 = (a[1].max(b[1]).max(cc[1]).ceil().max(0.0) as usize).min(res - 1);
                let mut covered = false;
                let mut hit = false;
                if x0 <= x1 && y0 <= y1 {
                    'px: for y in y0..=y1 {
                        for x in x0..=x1 {
                            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                            let w0 = ((b[0] - px) * (cc[1] - py) - (cc[0] - px) * (b[1] - py)) / area;
                            let w1 = ((cc[0] - px) * (a[1] - py) - (a[0] - px) * (cc[1] - py)) / area;
                            let w2 = 1.0 - w0 - w1;
                            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                                continue;
                            }
                            covered = true;
                            if test(x, y, w0 * a[2] + w1 * b[2] + w2 * cc[2]) {
                                hit = true;
                                break 'px;
                            }
                        }
                    }
                }
                if !covered {
                    // trop petit pour couvrir un centre de pixel : son centre
                    let (mx, my) = ((a[0] + b[0] + cc[0]) / 3.0, (a[1] + b[1] + cc[1]) / 3.0);
                    if mx >= 0.0 && my >= 0.0 && (mx as usize) < res && (my as usize) < res {
                        hit = test(mx as usize, my as usize, (a[2] + b[2] + cc[2]) / 3.0);
                    }
                }
                if pass == 1 && hit {
                    vis[ti] = true;
                }
            }
        }
    }
    if vis.iter().filter(|v| **v).count() * 20 < n {
        return vec![true; n];
    }
    vis
}

/// Plusieurs modèles du jeu assemblés en un seul (bâtiment complet) : chaque morceau placé (position,
/// orientation, échelles xy et z dans le repère commun), ses textures gardées ; la partie vue de l'extérieur est
/// recalculée sur l'ensemble.
pub fn merged_tex_model(w: &crate::gta::world::GameWorld, parts: &[(u32, V3, crate::math::Quat, f32, f32)]) -> Option<TexModel> {
    let mut out = TexModel { pos: Vec::new(), uv: Vec::new(), tris: Vec::new(), small: Vec::new(), big: Vec::new(), min: [0.0; 3], max: [0.0; 3], outside: Vec::new() };
    for (h, p, q, sxy, sz) in parts {
        let Some(m) = load_tex_model(w, *h, 32, 256) else { continue };
        let base = out.pos.len() as u32;
        let pb = out.small.len() as u16;
        out.pos.extend(m.pos.iter().map(|v| {
            let r = *p + q.rotate(v3(v[0] * sxy, v[1] * sxy, v[2] * sz));
            [r.x, r.y, r.z]
        }));
        out.uv.extend(m.uv.iter().copied());
        out.tris.extend(m.tris.iter().map(|(t, pi)| ([t[0] + base, t[1] + base, t[2] + base], pi + pb)));
        out.small.extend(m.small);
        out.big.extend(m.big);
    }
    if out.tris.is_empty() {
        return None;
    }
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for v in &out.pos {
        for k in 0..3 {
            mn[k] = mn[k].min(v[k]);
            mx[k] = mx[k].max(v[k]);
        }
    }
    out.min = mn;
    out.max = mx;
    out.outside = outside_mask(&out.pos, &out.tris, mn, mx);
    Some(out)
}

/// Comme `merged_tex_model`, en plus léger pour un intérieur (des centaines de morceaux) : chaque modèle n'est
/// chargé et ses petites textures décodées qu'une fois, même posé 30 fois ; pas de grande texture ni de calcul
/// de ce qui se voit de l'extérieur (tout est gardé).
pub fn merged_tex_model_lite(parts: &[(u32, V3, crate::math::Quat, f32, f32)], load: &mut dyn FnMut(u32) -> Option<std::sync::Arc<TexModel>>) -> Option<TexModel> {
    let mut out = TexModel { pos: Vec::new(), uv: Vec::new(), tris: Vec::new(), small: Vec::new(), big: Vec::new(), min: [0.0; 3], max: [0.0; 3], outside: Vec::new() };
    // modèle chargé et numéro de sa première partie dans `out.small`
    let mut cache: HashMap<u32, Option<(std::sync::Arc<TexModel>, u16)>> = HashMap::new();
    for (h, p, q, sxy, sz) in parts {
        let entry = cache.entry(*h).or_insert_with(|| {
            let m = load(*h)?;
            let base = out.small.len() as u16;
            out.small.extend(m.small.iter().cloned());
            out.big.extend(m.small.iter().map(|_| None));
            Some((m, base))
        });
        let Some((m, pb)) = entry.as_ref() else { continue };
        let base = out.pos.len() as u32;
        out.pos.extend(m.pos.iter().map(|v| {
            let r = *p + q.rotate(v3(v[0] * sxy, v[1] * sxy, v[2] * sz));
            [r.x, r.y, r.z]
        }));
        out.uv.extend(m.uv.iter().copied());
        out.tris.extend(m.tris.iter().map(|(t, pi)| ([t[0] + base, t[1] + base, t[2] + base], pi + pb)));
    }
    if out.tris.is_empty() {
        return None;
    }
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for v in &out.pos {
        for k in 0..3 {
            mn[k] = mn[k].min(v[k]);
            mx[k] = mx[k].max(v[k]);
        }
    }
    out.min = mn;
    out.max = mx;
    out.outside = vec![true; out.tris.len()];
    Some(out)
}

/// Vignette « maison de poupée » d'un intérieur : vue de trois quarts plongeante, seules les faces tournées vers
/// la caméra sont dessinées — les murs côté caméra et les plafonds (vus de dos) disparaissent, on voit les pièces
/// et les meubles. `max_tris` : les plus grands triangles gardés.
pub fn make_thumb_cutaway(m: &TexModel, frame: u64, max_tris: usize) -> Thumb {
    let c = [(m.min[0] + m.max[0]) * 0.5, (m.min[1] + m.max[1]) * 0.5, (m.min[2] + m.max[2]) * 0.5];
    let ext = ((m.max[0] - m.min[0]).powi(2) + (m.max[1] - m.min[1]).powi(2) + (m.max[2] - m.min[2]).powi(2)).sqrt().max(0.01);
    let (sy, cy) = 0.6f32.sin_cos();
    let (sp, cp) = CUTAWAY_PITCH.sin_cos();
    let pts: Vec<[f32; 3]> = m
        .pos
        .iter()
        .map(|p| {
            let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
            let rx = x * cy - y * sy;
            let ry = x * sy + y * cy;
            [rx / ext * 1.9, (z * cp - ry * sp) / ext * 1.9, ry * cp + z * sp]
        })
        .collect();
    let light = v3(-0.4, 0.6, -0.7).norm();
    let front = std::env::var("DW_CUT_SIGN").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(CUTAWAY_FRONT);
    // coupe au-dessus du sol principal (comme la touche C) : sol = tranche de 50 cm où il y a le plus de surface
    // tournée vers le haut ; ce qui est entièrement plus haut que sol + CUTAWAY_HEIGHT n'est pas dessiné
    // (plafonds, lampes, étages du dessus)
    let floor = {
        let mut bins: HashMap<i32, f32> = HashMap::new();
        for (t, _) in &m.tris {
            let (a, b, cc) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
            let n = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]).cross(v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]));
            let area = n.len() * 0.5;
            if area > 1e-4 && n.z / (2.0 * area) > 0.9 {
                *bins.entry(((a[2] + b[2] + cc[2]) / 3.0 / 0.5).floor() as i32).or_default() += area;
            }
        }
        if std::env::var("DW_DEBUG_CUT").is_ok() {
            let mut v: Vec<(i32, f32)> = bins.iter().map(|(k, a)| (*k, *a)).collect();
            v.sort_by_key(|x| x.0);
            println!("coupe : z de {:.1} à {:.1}, surfaces vers le haut par tranche : {:?}", m.min[2], m.max[2], v.iter().filter(|x| x.1 > 5.0).map(|x| (x.0 as f32 * 0.5, x.1 as i32)).collect::<Vec<_>>());
        }
        bins.into_iter().max_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal)).map(|(k, _)| k as f32 * 0.5)
    };
    let cut = floor.map(|f| f + 0.5 + CUTAWAY_HEIGHT).unwrap_or(f32::MAX);
    let mut tris: Vec<(f32, f32, [Pos2; 3], [Color32; 3])> = Vec::with_capacity(m.tris.len());
    for (t, part) in &m.tris {
        if t.iter().all(|i| m.pos[*i as usize][2] > cut) {
            continue;
        }
        let (a, b, cc) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        // aire signée à l'écran (y vers le haut) : négative = face vue de dos
        let signed = (b[0] - a[0]) * (cc[1] - a[1]) - (cc[0] - a[0]) * (b[1] - a[1]);
        if signed * front <= 0.0 {
            continue;
        }
        let n = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]).cross(v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2])).norm();
        let k = (0.62 + 0.55 * n.dot(light).abs()).min(1.15);
        let img = m.small.get(*part as usize).and_then(|x| x.as_ref());
        let mut cols = [Color32::BLACK; 3];
        let mut transparent = 0;
        for (j, vi) in t.iter().enumerate() {
            let s = match img {
                Some(im) => {
                    let uv = m.uv[*vi as usize];
                    im.sample(uv[0], uv[1])
                }
                None => [184, 184, 184, 255],
            };
            if s[3] < 60 {
                transparent += 1;
            }
            cols[j] = Color32::from_rgb((s[0] as f32 * k).min(255.0) as u8, (s[1] as f32 * k).min(255.0) as u8, (s[2] as f32 * k).min(255.0) as u8);
        }
        if transparent == 3 {
            continue;
        }
        // sol principal : dessiné avant tout le reste (un grand triangle de sol trié par distance passerait
        // par-dessus les meubles posés au fond de la pièce)
        let (wa, wb, wc) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
        let wn = v3(wb[0] - wa[0], wb[1] - wa[1], wb[2] - wa[2]).cross(v3(wc[0] - wa[0], wc[1] - wa[1], wc[2] - wa[2])).norm();
        let wz = (wa[2] + wb[2] + wc[2]) / 3.0;
        let is_floor = wn.z > 0.8 && floor.map(|f| wz > f - 0.3 && wz < f + 0.8).unwrap_or(false);
        let depth = if is_floor { f32::MAX } else { (a[2] + b[2] + cc[2]) / 3.0 };
        tris.push((signed.abs(), depth, [Pos2::new(a[0], -a[1]), Pos2::new(b[0], -b[1]), Pos2::new(cc[0], -cc[1])], cols));
    }
    tris.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    tris.truncate(max_tris);
    tris.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
    // cadrage : ce qui est dessiné remplit le cadre (un intérieur vu en plongée est plus large que haut)
    let (mut lo, mut hi) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
    for t in &tris {
        for q in &t.2 {
            lo = lo.min(*q);
            hi = hi.max(*q);
        }
    }
    let span = (hi.x - lo.x).max(hi.y - lo.y);
    if span > 1e-6 && span.is_finite() {
        let mid = Pos2::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
        let k = 1.9 / span;
        for t in tris.iter_mut() {
            for q in t.2.iter_mut() {
                *q = Pos2::new((q.x - mid.x) * k, (q.y - mid.y) * k);
            }
        }
    }
    Thumb { tris: tris.into_iter().map(|t| (t.2, t.3)).collect(), last: frame, dims: v3(m.max[0] - m.min[0], m.max[1] - m.min[1], m.max[2] - m.min[2]) }
}

/// Hauteur gardée au-dessus du sol principal dans la vue « maison de poupée » (m) — **estimé** (sous les plafonds
/// et les lampes des garages et appartements du jeu).
const CUTAWAY_HEIGHT: f32 = 2.5;
/// Plongée de la vue « maison de poupée » (radians) — **estimé**.
const CUTAWAY_PITCH: f32 = 0.95;
/// Sens des faces avant à l'écran : +1 si les triangles du jeu sont dans le sens inverse des aiguilles d'une
/// montre vus de face, −1 sinon (relevé sur les captures, voir la variable DW_CUT_SIGN des tests).
const CUTAWAY_FRONT: f32 = -1.0;

/// Aperçu texturé : le modèle et ses textures envoyées à egui.
pub struct TexPreview {
    pub model: TexModel,
    pub tex: Vec<Option<egui::TextureHandle>>,
}

/// Charge un modèle du jeu avec ses textures (comme la vue 3D : textures du modèle, du dictionnaire et des parents).
pub fn load_tex_model(w: &crate::gta::world::GameWorld, h: u32, small_px: u32, big_px: u32) -> Option<TexModel> {
    load_tex_model_src(&w.model_source(), h, small_px, big_px, &mut Vec::new(), false)
}

/// Comme `load_tex_model`, depuis une source de modèles (utilisable dans un autre fil) et avec un cache des
/// dictionnaires de textures partagé entre plusieurs chargements.
pub fn load_tex_model_src(src: &crate::gta::loader::ModelSource, h: u32, small_px: u32, big_px: u32, ytd_cache: &mut Vec<(u32, std::sync::Arc<crate::gta::rpf::Resource>)>, opaque_only: bool) -> Option<TexModel> {
    let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
    let l = src.load(h, &claimed, ytd_cache);
    let m = l.model?;
    if m.verts.is_empty() || m.idx.len() < 3 {
        return None;
    }
    let texmap: HashMap<u64, &crate::gta::drawable::TexData> = l.textures.iter().map(|(k, t)| (*k, t)).collect();
    let mut small = Vec::new();
    let mut big = Vec::new();
    let mut tris = Vec::new();
    for (gi, g) in m.geoms.iter().enumerate() {
        let key = l.part_keys.get(gi).copied().unwrap_or(0);
        let t = texmap.get(&key);
        small.push(t.and_then(|t| crate::texdecode::decode_level(t, small_px)));
        big.push(if big_px > 0 { t.and_then(|t| crate::texdecode::decode_level(t, big_px)) } else { None });
        if std::env::var("DW_DEBUG_MAGENTA").is_ok() {
            if let Some(im) = small.last().and_then(|x| x.as_ref()) {
                let n = (im.px.len() / 4).max(1) as u64;
                let (r, gg, b) = im.px.chunks_exact(4).fold((0u64, 0u64, 0u64), |a, p| (a.0 + p[0] as u64, a.1 + p[1] as u64, a.2 + p[2] as u64));
                let (r, gg, b) = (r / n, gg / n, b / n);
                if r > 150 && b > 150 && gg < 100 {
                    println!("magenta : modèle {h:08x} partie {gi} terrain {} blend {} tex {:08x} couleur ({r} {gg} {b})", g.terrain, g.blend, g.tex);
                }
            }
        }
        // opaque_only : sans vitres, décalques ni parties transparentes (vignettes d'intérieurs)
        if opaque_only && (g.blend != 0 || g.glass) {
            continue;
        }
        let end = (g.first + g.count).min(m.idx.len() as u32);
        let mut i = g.first;
        while i + 2 < end {
            tris.push(([m.idx[i as usize], m.idx[i as usize + 1], m.idx[i as usize + 2]], gi as u16));
            i += 3;
        }
    }
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for v in &m.verts {
        for k in 0..3 {
            mn[k] = mn[k].min(v.pos[k]);
            mx[k] = mx[k].max(v.pos[k]);
        }
    }
    let pos: Vec<[f32; 3]> = m.verts.iter().map(|v| v.pos).collect();
    let outside = outside_mask(&pos, &tris, mn, mx);
    Some(TexModel { pos, uv: m.verts.iter().map(|v| v.uv).collect(), tris, small, big, min: mn, max: mx, outside })
}

/// Vignette texturée : vue de trois quarts, couleur de la texture à chaque coin des triangles.
pub fn make_thumb_tex(m: &TexModel, frame: u64) -> Thumb {
    let c = [(m.min[0] + m.max[0]) * 0.5, (m.min[1] + m.max[1]) * 0.5, (m.min[2] + m.max[2]) * 0.5];
    let ext = ((m.max[0] - m.min[0]).powi(2) + (m.max[1] - m.min[1]).powi(2) + (m.max[2] - m.min[2]).powi(2)).sqrt().max(0.01);
    let (sy, cy) = 0.6f32.sin_cos();
    let (sp, cp) = 0.45f32.sin_cos();
    let pts: Vec<[f32; 3]> = m
        .pos
        .iter()
        .map(|p| {
            let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
            let rx = x * cy - y * sy;
            let ry = x * sy + y * cy;
            [rx / ext * 1.9, (z * cp - ry * sp) / ext * 1.9, ry * cp + z * sp]
        })
        .collect();
    let light = v3(-0.4, 0.6, -0.7).norm();
    let mut tris: Vec<(f32, f32, [Pos2; 3], [Color32; 3])> = Vec::with_capacity(m.tris.len());
    for (ti, (t, part)) in m.tris.iter().enumerate() {
        // l'intérieur (caché derrière la façade) n'est pas dessiné
        if !m.outside.get(ti).copied().unwrap_or(true) {
            continue;
        }
        let (a, b, cc) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        let n = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]).cross(v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2])).norm();
        let k = (0.62 + 0.55 * n.dot(light).abs()).min(1.15);
        let img = m.small.get(*part as usize).and_then(|x| x.as_ref());
        let mut cols = [Color32::BLACK; 3];
        let mut transparent = 0;
        for (j, vi) in t.iter().enumerate() {
            let s = match img {
                Some(im) => {
                    let uv = m.uv[*vi as usize];
                    im.sample(uv[0], uv[1])
                }
                None => [184, 184, 184, 255],
            };
            if s[3] < 60 {
                transparent += 1;
            }
            cols[j] = Color32::from_rgb((s[0] as f32 * k).min(255.0) as u8, (s[1] as f32 * k).min(255.0) as u8, (s[2] as f32 * k).min(255.0) as u8);
        }
        // feuillages, grilles : les coins transparents de la texture ne sont pas dessinés
        if transparent == 3 {
            continue;
        }
        let area = ((b[0] - a[0]) * (cc[1] - a[1]) - (cc[0] - a[0]) * (b[1] - a[1])).abs();
        tris.push((area, (a[2] + b[2] + cc[2]) / 3.0, [Pos2::new(a[0], -a[1]), Pos2::new(b[0], -b[1]), Pos2::new(cc[0], -cc[1])], cols));
    }
    tris.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    tris.truncate(THUMB_TRIS);
    tris.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
    Thumb { tris: tris.into_iter().map(|t| (t.2, t.3)).collect(), last: frame, dims: v3(m.max[0] - m.min[0], m.max[1] - m.min[1], m.max[2] - m.min[2]) }
}

/// Catégories de la bibliothèque : nom, info-bulle, mots cherchés dans le nom du prop (n'importe lequel).
/// Classement par mots du nom — **estimé** (le jeu ne range pas ses props par catégorie).
pub const CATEGORIES: [(&str, &str, &[&str]); 15] = [
    ("Tous", "Tous les props", &[]),
    ("Meubles", "Chaises, canapés, tables, lits, étagères", &["chair", "sofa", "couch", "table", "desk", "bed_", "_bed", "shelf", "cabinet", "stool", "bench", "armchair", "wardrobe", "drawer", "seat"]),
    ("Cuisine / bain", "Cuisine, salle de bain, vaisselle", &["kitchen", "fridge", "sink", "toilet", "bath", "shower", "cooker", "oven", "microwave", "kettle", "mug", "cup_", "plate", "pan_", "toaster"]),
    ("Éclairage", "Lampes, lampadaires, néons", &["light", "lamp", "lantern", "neon", "spotlight", "bulb", "chandelier", "torch"]),
    ("Rue", "Poubelles, bornes, panneaux, feux, cabines", &["bin", "dumpster", "hydrant", "bollard", "sign", "traffic", "parking", "meter", "phonebox", "busstop", "bus_stop", "news", "mailbox", "postbox", "barrier", "streetlight", "atm", "vend"]),
    ("Végétation", "Arbres, buissons, plantes, rochers", &["tree", "bush", "plant", "palm", "grass", "flower", "fern", "hedge", "cactus", "veg_", "rock", "log_", "joshua"]),
    ("Clôtures / murs", "Clôtures, murets, portails, rambardes", &["fnc", "fence", "wall", "gate", "railing", "rail_", "barbed", "barricade"]),
    ("Chantier", "Échafaudages, caisses, palettes, conteneurs, tonneaux", &["const", "scaff", "crate", "pallet", "container", "barrel", "drum", "pipe", "plank", "girder", "cement", "ladder", "cone", "skip", "tarp", "sandbag"]),
    ("Garage / auto", "Pneus, outils, pompes à essence", &["tyre", "tire", "wheel", "jack", "engine", "garage", "fuel", "gas_pump", "gaspump", "oil", "toolchest", "car_"]),
    ("Électronique", "Écrans, ordinateurs, téléphones, caméras", &["tv", "monitor", "computer", "laptop", "pc_", "phone", "speaker", "radio", "console", "screen", "keyboard", "cctv", "cam_", "printer"]),
    ("Sport / loisirs", "Muscu, ballons, plage, piscine", &["gym", "weight", "ball", "golf", "tennis", "basket", "skate", "surf", "bike", "pool", "beach", "umbrella", "parasol", "sunlounger", "bbq", "picnic"]),
    ("Nourriture", "Nourriture et boissons", &["food", "drink", "bottle", "beer", "can_", "burger", "pizza", "fruit", "cola", "coffee", "wine", "whiskey", "cake", "bread", "donut"]),
    ("Bureau", "Papiers, dossiers, tableaux, fournitures", &["office", "paper", "folder", "file", "copier", "whiteboard", "binder", "pen_", "notepad", "clipboard", "book"]),
    ("Décoration", "Vases, cadres, statues, tapis, rideaux, horloges", &["vase", "frame", "art_", "statue", "rug", "curtain", "clock", "mirror", "poster", "pic_", "painting", "candle", "ornament"]),
    ("Effets", "Props qui portent un effet du jeu (feu, fumée, vapeur…)", &["fire", "smoke", "steam", "fountain", "_fx", "flare", "sprinkler"]),
];

/// Vrai si le nom du prop contient un des mots de la catégorie (ou si la catégorie est « Tous »).
pub fn in_category(name: &str, words: &[&str]) -> bool {
    words.is_empty() || words.iter().any(|w| name.contains(w))
}

pub struct Library {
    /// aperçu texturé en cours (prop, modèle)
    pub preview: Option<(u32, Option<TexPreview>)>,
    pub open: bool,
    pub thumbs: HashMap<u32, Thumb>,
    /// modèles sans aperçu possible (introuvables ou illisibles)
    pub missing: std::collections::HashSet<u32>,
    pub size: f32,
    pub frame: u64,
    /// vignettes encore à préparer parmi celles affichées
    pub pending: usize,
    /// prop qu'on glisse vers la carte
    pub dragging: Option<u32>,
    pub hovered: Option<u32>,
}

impl Library {
    pub fn new() -> Library {
        Library {
            preview: None,
            open: std::env::var("DW_LIBRARY").is_ok(),
            thumbs: HashMap::new(),
            missing: std::collections::HashSet::new(),
            size: 96.0,
            frame: 0,
            pending: 0,
            dragging: None,
            hovered: None,
        }
    }
}

/// Triangles gardés par vignette (les plus grands à l'écran) — estimé.
const THUMB_TRIS: usize = 1800;
/// Vignettes préparées par image (chargement du modèle compris) — estimé.
const THUMBS_PER_FRAME: usize = 3;
/// Vignettes gardées en mémoire — estimé.
const THUMB_CACHE: usize = 900;

/// Prépare la vignette d'un modèle : vue de trois quarts un peu plongeante, comme l'aperçu.
fn make_thumb(m: &crate::gta::drawable::Mesh, frame: u64) -> Thumb {
    let c = [(m.min[0] + m.max[0]) * 0.5, (m.min[1] + m.max[1]) * 0.5, (m.min[2] + m.max[2]) * 0.5];
    let ext = ((m.max[0] - m.min[0]).powi(2) + (m.max[1] - m.min[1]).powi(2) + (m.max[2] - m.min[2]).powi(2)).sqrt().max(0.01);
    let (sy, cy) = 0.6f32.sin_cos();
    let (sp, cp) = 0.45f32.sin_cos();
    let pts: Vec<[f32; 3]> = m
        .pos
        .iter()
        .map(|p| {
            let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
            let rx = x * cy - y * sy;
            let ry = x * sy + y * cy;
            [rx / ext * 1.9, (z * cp - ry * sp) / ext * 1.9, ry * cp + z * sp]
        })
        .collect();
    let light = v3(-0.4, 0.6, -0.7).norm();
    let mut tris: Vec<(f32, f32, [Pos2; 3], [Color32; 3])> = m
        .tris
        .iter()
        .map(|t| {
            let (a, b, cc) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            let n = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]).cross(v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2])).norm();
            let k = 0.35 + 0.65 * n.dot(light).abs();
            let col = Color32::from_rgb((205.0 * k) as u8, (185.0 * k) as u8, (150.0 * k) as u8);
            let area = ((b[0] - a[0]) * (cc[1] - a[1]) - (cc[0] - a[0]) * (b[1] - a[1])).abs();
            (area, (a[2] + b[2] + cc[2]) / 3.0, [Pos2::new(a[0], -a[1]), Pos2::new(b[0], -b[1]), Pos2::new(cc[0], -cc[1])], [col; 3])
        })
        .collect();
    // les plus grands triangles d'abord, puis du plus loin au plus proche pour le dessin
    tris.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    tris.truncate(THUMB_TRIS);
    tris.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
    Thumb { tris: tris.into_iter().map(|t| (t.2, t.3)).collect(), last: frame, dims: v3(m.max[0] - m.min[0], m.max[1] - m.min[1], m.max[2] - m.min[2]) }
}

pub fn draw_thumb(painter: &egui::Painter, rect: egui::Rect, t: &Thumb) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.5;
    let mut mesh = egui::Mesh::default();
    for (p, col) in &t.tris {
        let base = mesh.vertices.len() as u32;
        for (q, cc) in p.iter().zip(col.iter()) {
            mesh.colored_vertex(Pos2::new(c.x + q.x * s, c.y + q.y * s), *cc);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

impl DwApp {
    /// Pose un prop de la bibliothèque à un point du monde (au centre de la vue s'il n'y en a pas).
    /// Point d'apparition d'un nouvel objet, comme CodeWalker : devant la caméra, position + direction de vue ×
    /// distance (ProjectForm::GetSpawnPos 0x06000dfb) ; distance 5 m, ou 2,5 × le rayon de l'objet
    /// (ProjectForm::NewEntity 0x06000d3e, IL_0038–IL_004d) — ici le plus grand des deux, pour qu'un gros prop
    /// ne se retrouve pas sur la caméra. En vue carte : le centre de la vue.
    pub fn spawn_pos(&self, name: &str) -> V3 {
        if self.cam.map_view {
            return self.cam.target;
        }
        let h = crate::render::arch_hash(name);
        let r = self.game.world.as_ref().and_then(|w| w.archetypes.get(&h)).map(|a| a.radius).unwrap_or(0.0);
        let d = 5.0f32.max(r * 2.5);
        self.cam.eye + self.cam.fwd * d
    }

    pub fn library_place(&mut self, name: &str, at: Option<V3>) {
        let g = at.or(self.mouse_ground).unwrap_or(self.cam.target);
        self.push_undo();
        let id = self.place_prop(name, g, None);
        self.set_selection(vec![id]);
        self.snap_if_shift(&[id]);
        self.place_arch = name.to_string();
        self.status = format!("« {name} » posé à {:.1} {:.1} {:.1}.", g.x, g.y, g.z);
    }

    /// Fenêtre « Bibliothèque de props ».
    pub fn library_window(&mut self, ctx: &egui::Context) {
        if self.dock_phase == 1 {
            self.library.frame += 1;
        }
        if !self.library.open {
            self.library.pending = 0;
            return;
        }
        let mut open = self.library.open;
        let mut place_now: Option<String> = None;
        let screen = ctx.screen_rect();
        let max_w = (screen.width() - 60.0).max(500.0);
        self.show_panel(ctx, "bibliotheque", "Bibliothèque de props", crate::docking::Dock::Floating, [980.0_f32.min(max_w), 640.0], &mut open, |s, ui| {
                if s.game.world.is_none() {
                    ui.label("Charge d'abord le jeu (onglet Jeu) pour voir tous ses props.");
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(&mut s.game.cat_filter).hint_text("Chercher : chair, bench, tree, light, door…").desired_width(260.0));
                    for (i, l) in ["Tous", "Jeu de base", "Cayo Perico", "DLC / mises à jour"].iter().enumerate() {
                        if ui.selectable_label(s.game.cat_source == i as u8, *l).clicked() {
                            s.game.cat_source = i as u8;
                        }
                    }
                    ui.checkbox(&mut s.game.cat_props_only, "Props seulement");
                    ui.label("Taille");
                    ui.add(egui::Slider::new(&mut s.library.size, 64.0..=180.0).show_value(false));
                });
                // catégories (par défaut : tous)
                ui.horizontal_wrapped(|ui| {
                    for (i, (n, tip, _)) in CATEGORIES.iter().enumerate() {
                        let on = s.game.cat_group == i as u8;
                        let txt = if on { RichText::new(*n).strong().color(Color32::BLACK) } else { RichText::new(*n) };
                        let b = egui::Button::new(txt).fill(if on { Color32::from_rgb(255, 170, 60) } else { Color32::from_gray(45) }).corner_radius(10.0);
                        if ui.add(b).on_hover_text(*tip).clicked() {
                            s.game.cat_group = i as u8;
                        }
                    }
                });
                s.refresh_catalog_pub();
                let total = s.game.cat_view.len();
                ui.label(RichText::new(format!("{total} modèles · clic : choisir pour l'outil Poser · double-clic : poser devant la caméra · glisser sur la carte : poser à cet endroit")).small().weak());
                ui.separator();
                // la grille prend la largeur de la fenêtre (sans la faire grandir), l'aperçu à droite
                let avail = egui::vec2(ui.available_width().min(max_w - 20.0), ui.available_height().max(300.0));
                let side_w = 300.0f32.min(avail.x * 0.35);
                ui.horizontal_top(|ui| {
                    // grille
                    let grid_w = (avail.x - side_w - 16.0).max(200.0);
                    let cell = s.library.size;
                    let cols = ((grid_w / (cell + 8.0)).floor() as usize).max(1);
                    let rows = total.div_ceil(cols);
                    let row_h = cell + 26.0;
                    let mut todo: Vec<u32> = Vec::new();
                    let mut hovered = None;
                    let place_hash = crate::gta::crypto::joaat(&s.place_arch);
                    ui.allocate_ui_with_layout(egui::vec2(grid_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(grid_w);
                        ui.set_height(avail.y);
                        egui::ScrollArea::vertical().id_salt("lib_grid").max_width(grid_w).auto_shrink([false; 2]).show_rows(ui, row_h, rows, |ui, range| {
                            for r in range {
                                ui.horizontal(|ui| {
                                    for c in 0..cols {
                                        let i = r * cols + c;
                                        if i >= total {
                                            break;
                                        }
                                        let h = s.game.cat_view[i];
                                        let name = s.game.world.as_ref().map(|w| w.name_of(h)).unwrap_or_default();
                                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell + 4.0, row_h - 4.0), Sense::click_and_drag());
                                        let img = egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2(cell, cell));
                                        let p = ui.painter_at(rect);
                                        let sel = place_hash == h;
                                        p.rect_filled(img, 5.0, if resp.hovered() { Color32::from_rgb(44, 50, 60) } else { Color32::from_rgb(30, 33, 39) });
                                        match s.library.thumbs.get_mut(&h) {
                                            Some(t) => {
                                                t.last = s.library.frame;
                                                draw_thumb(&p, img.shrink(6.0), t);
                                            }
                                            None if s.library.missing.contains(&h) => {
                                                p.text(img.center(), egui::Align2::CENTER_CENTER, "pas d'aperçu", egui::FontId::proportional(10.0), Color32::GRAY);
                                            }
                                            None => {
                                                todo.push(h);
                                                p.text(img.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), Color32::GRAY);
                                            }
                                        }
                                        if sel {
                                            p.rect_stroke(img, 5.0, Stroke::new(2.0_f32, Color32::from_rgb(255, 170, 60)), egui::StrokeKind::Inside);
                                        }
                                        let short: String = if name.chars().count() > 18 { format!("{}…", name.chars().take(17).collect::<String>()) } else { name.clone() };
                                        p.text(egui::pos2(img.center().x, img.max.y + 3.0), egui::Align2::CENTER_TOP, short, egui::FontId::proportional(11.0), Color32::from_gray(200));
                                        let resp = resp.on_hover_text(&name);
                                        if resp.hovered() {
                                            hovered = Some(h);
                                        }
                                        if resp.double_clicked() {
                                            place_now = Some(name.clone());
                                        } else if resp.clicked() {
                                            s.place_arch = name.clone();
                                            s.tool = Tool::Place;
                                            s.status = format!("« {name} » choisi : clique sur la carte pour le poser (outil Poser).");
                                        }
                                        if resp.drag_started() {
                                            s.library.dragging = Some(h);
                                        }
                                    }
                                });
                            }
                        });
                    });
                    if hovered.is_some() {
                        s.library.hovered = hovered;
                    }
                    // vignettes à préparer : quelques-unes par image pour ne pas ralentir
                    s.library.pending = todo.len();
                    for h in todo.into_iter().take(THUMBS_PER_FRAME) {
                        let tm = s.game.world.as_ref().and_then(|w| load_tex_model(w, h, 32, 0));
                        let m = if tm.is_none() { s.game.world.as_ref().and_then(|w| w.load_model(h, 30000)) } else { None };
                        match (tm, m) {
                            (Some(tm), _) => {
                                let t = make_thumb_tex(&tm, s.library.frame);
                                s.library.thumbs.insert(h, t);
                            }
                            (None, Some(m)) => {
                                let t = make_thumb(&m, s.library.frame);
                                s.library.thumbs.insert(h, t);
                            }
                            (None, None) => {
                                s.library.missing.insert(h);
                            }
                        }
                    }
                    if s.library.pending > 0 {
                        ctx.request_repaint();
                    }
                    if s.library.thumbs.len() > THUMB_CACHE {
                        let mut v: Vec<(u64, u32)> = s.library.thumbs.iter().map(|(k, t)| (t.last, *k)).collect();
                        v.sort();
                        for (_, k) in v.into_iter().take(s.library.thumbs.len() - THUMB_CACHE) {
                            s.library.thumbs.remove(&k);
                        }
                    }
                    // aperçu en direct du prop survolé (sinon celui choisi)
                    ui.vertical(|ui| {
                        ui.set_width(side_w);
                        let show = s.library.hovered.or(Some(place_hash)).unwrap_or(0);
                        let name = s.game.world.as_ref().map(|w| w.name_of(show)).unwrap_or_default();
                        ui.label(RichText::new(&name).strong());
                        s.textured_preview(ui, show, &name);
                        if let Some(t) = s.library.thumbs.get(&show) {
                            ui.label(format!("{:.2} × {:.2} × {:.2} m", t.dims.x, t.dims.y, t.dims.z));
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Choisir pour Poser").clicked() {
                                s.place_arch = name.clone();
                                s.tool = Tool::Place;
                            }
                            if ui.button("Poser devant la caméra").on_hover_text("Comme « nouvel objet » dans CodeWalker : 5 m devant la caméra (plus pour un gros prop)").clicked() {
                                place_now = Some(name.clone());
                            }
                            if ui.button("Modifier dans Blender").clicked() {
                                s.edit_in_blender(&name);
                            }
                        });
                    });
                });
            });
        self.library.open = open;
        if let Some(n) = place_now {
            let at = self.spawn_pos(&n);
            self.library_place(&n, Some(at));
        }
        // glisser une vignette sur la carte : fantôme qui suit la souris, pose au lâcher
        if let Some(h) = self.library.dragging.filter(|_| self.dock_phase == 1) {
            let name = self.game.world.as_ref().map(|w| w.name_of(h)).unwrap_or_default();
            if let Some(p) = ctx.pointer_hover_pos() {
                egui::Area::new(egui::Id::new("lib_drag")).fixed_pos(p + egui::vec2(12.0, 12.0)).order(egui::Order::Tooltip).show(ctx, |ui| {
                    ui.label(RichText::new(format!("» {name}")).background_color(Color32::from_rgba_unmultiplied(0, 0, 0, 170)).color(Color32::from_rgb(255, 200, 90)));
                });
            }
            if ctx.input(|i| i.pointer.any_released()) {
                self.library.dragging = None;
                if let Some(p) = ctx.pointer_hover_pos().filter(|p| self.cam.rect.contains(*p)) {
                    if !ctx.is_pointer_over_area() {
                        // sol sous le point de lâcher (relief du jeu, sinon plan du sol)
                        let at = self.game_ground(p).or_else(|| self.cam.ground_point(p, self.ground_z));
                        self.library_place(&name, at);
                    }
                }
            }
        }
    }
}

impl DwApp {
    /// Prépare quelques vignettes demandées par un autre panneau (catalogue des meubles, meubler.rs) : même cache
    /// que la bibliothèque, quelques-unes par image pour ne pas ralentir.
    pub fn library_make_thumbs(&mut self, todo: &[u32]) {
        for &h in todo.iter().take(THUMBS_PER_FRAME) {
            let tm = self.game.world.as_ref().and_then(|w| load_tex_model(w, h, 32, 0));
            let m = if tm.is_none() { self.game.world.as_ref().and_then(|w| w.load_model(h, 30000)) } else { None };
            match (tm, m) {
                (Some(tm), _) => {
                    let t = make_thumb_tex(&tm, self.library.frame);
                    self.library.thumbs.insert(h, t);
                }
                (None, Some(m)) => {
                    let t = make_thumb(&m, self.library.frame);
                    self.library.thumbs.insert(h, t);
                }
                (None, None) => {
                    self.library.missing.insert(h);
                }
            }
        }
        if self.library.thumbs.len() > THUMB_CACHE {
            let mut v: Vec<(u64, u32)> = self.library.thumbs.iter().map(|(k, t)| (t.last, *k)).collect();
            v.sort();
            for (_, k) in v.into_iter().take(self.library.thumbs.len() - THUMB_CACHE) {
                self.library.thumbs.remove(&k);
            }
        }
    }

    /// Grand aperçu qui tourne, avec les vraies textures du jeu posées sur le modèle.
    pub fn textured_preview(&mut self, ui: &mut egui::Ui, h: u32, name: &str) {
        self.textured_preview_with(ui, h, name, &|w| load_tex_model(w, h, 32, 256));
    }

    /// Aperçu tournant d'un modèle donné par `load` (gardé tant que `h` ne change pas).
    pub fn textured_preview_with(&mut self, ui: &mut egui::Ui, h: u32, name: &str, load: &dyn Fn(&crate::gta::world::GameWorld) -> Option<TexModel>) {
        if self.library.preview.as_ref().map(|p| p.0) != Some(h) {
            let tm = self.game.world.as_ref().and_then(|w| load(w));
            let ctx = ui.ctx().clone();
            let tp = tm.map(|model| {
                let tex = model
                    .big
                    .iter()
                    .enumerate()
                    .map(|(i, im)| {
                        im.as_ref().map(|im| {
                            let img = egui::ColorImage::from_rgba_unmultiplied([im.w, im.h], &im.px);
                            ctx.load_texture(format!("dw_prev_{h:08x}_{i}"), img, egui::TextureOptions::LINEAR_REPEAT)
                        })
                    })
                    .collect();
                TexPreview { model, tex }
            });
            self.library.preview = Some((h, tp));
        }
        let width = ui.available_width().min(330.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 240.0), Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::from_rgb(28, 31, 36));
        if resp.dragged() {
            let d = resp.drag_delta();
            self.game.preview_yaw += d.x * 0.012;
            self.game.preview_pitch = (self.game.preview_pitch + d.y * 0.01).clamp(-0.2, 1.4);
        } else {
            self.game.preview_yaw += 0.006;
            ui.ctx().request_repaint();
        }
        let Some((_, Some(tp))) = &self.library.preview else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Pas d'aperçu pour ce modèle", egui::FontId::proportional(13.0), Color32::GRAY);
            return;
        };
        let m = &tp.model;
        let c = [(m.min[0] + m.max[0]) * 0.5, (m.min[1] + m.max[1]) * 0.5, (m.min[2] + m.max[2]) * 0.5];
        let ext = ((m.max[0] - m.min[0]).powi(2) + (m.max[1] - m.min[1]).powi(2) + (m.max[2] - m.min[2]).powi(2)).sqrt().max(0.01);
        let scale = rect.height().min(rect.width()) * 0.82 / ext;
        let (sy, cy) = self.game.preview_yaw.sin_cos();
        let (sp, cp) = self.game.preview_pitch.sin_cos();
        let pts: Vec<[f32; 3]> = m
            .pos
            .iter()
            .map(|p| {
                let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
                let rx = x * cy - y * sy;
                let ry = x * sy + y * cy;
                [rx, z * cp - ry * sp, ry * cp + z * sp]
            })
            .collect();
        let light = v3(-0.4, 0.6, -0.7).norm();
        let inside = self.game.preview_inside;
        let mut order: Vec<(f32, usize, u8)> = m
            .tris
            .iter()
            .enumerate()
            .filter(|(i, _)| inside || m.outside.get(*i).copied().unwrap_or(true))
            .map(|(i, (t, _))| {
                let (a, b, cc) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
                let n = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]).cross(v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2])).norm();
                let k = (0.62 + 0.55 * n.dot(light).abs()).min(1.15);
                ((a[2] + b[2] + cc[2]) / 3.0, i, (k.min(1.0) * 255.0) as u8)
            })
            .collect();
        order.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        let ctr = rect.center();
        let to_s = |p: [f32; 3]| egui::pos2(ctr.x + p[0] * scale, ctr.y - p[1] * scale);
        // triangles du plus loin au plus proche, regroupés tant qu'ils partagent la même texture
        let mut mesh = egui::Mesh::default();
        let mut cur: Option<egui::TextureId> = None;
        for (_, ti, k) in order {
            let (t, part) = &m.tris[ti];
            let tex = tp.tex.get(*part as usize).and_then(|x| x.as_ref()).map(|t| t.id()).unwrap_or(egui::TextureId::default());
            if cur != Some(tex) {
                if !mesh.vertices.is_empty() {
                    painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
                }
                mesh = egui::Mesh::with_texture(tex);
                cur = Some(tex);
            }
            let textured = tex != egui::TextureId::default();
            let g = if textured { k } else { (k as f32 * 0.72) as u8 };
            let col = Color32::from_rgb(g, g, g);
            let base = mesh.vertices.len() as u32;
            for vi in t {
                let uv = if textured { egui::pos2(m.uv[*vi as usize][0], m.uv[*vi as usize][1]) } else { egui::epaint::WHITE_UV };
                mesh.vertices.push(egui::epaint::Vertex { pos: to_s(pts[*vi as usize]), uv, color: col });
            }
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
        let sz = [m.max[0] - m.min[0], m.max[1] - m.min[1], m.max[2] - m.min[2]];
        painter.text(rect.left_bottom() + egui::vec2(8.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{name}  ·  {:.2} × {:.2} × {:.2} m  ·  {} triangles", sz[0], sz[1], sz[2], m.tris.len()), egui::FontId::proportional(11.5), Color32::from_gray(220));
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, Color32::from_gray(70)), egui::StrokeKind::Inside);
        let hidden = m.outside.iter().filter(|v| !**v).count();
        if hidden > 0 {
            ui.checkbox(&mut self.game.preview_inside, format!("Voir aussi l'intérieur ({hidden} triangles cachés derrière la façade)"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(pos: &mut Vec<[f32; 3]>, tris: &mut Vec<([u32; 3], u16)>, c: f32, h: f32) {
        let b = pos.len() as u32;
        for i in 0..8 {
            pos.push([if i & 1 == 0 { c - h } else { c + h }, if i & 2 == 0 { c - h } else { c + h }, if i & 4 == 0 { c - h } else { c + h }]);
        }
        for f in [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]] {
            tris.push(([b + f[0], b + f[1], b + f[2]], 0));
            tris.push(([b + f[0], b + f[2], b + f[3]], 0));
        }
    }

    #[test]
    fn inside_of_a_closed_box_is_hidden() {
        let (mut pos, mut tris) = (Vec::new(), Vec::new());
        cube(&mut pos, &mut tris, 0.0, 10.0);
        cube(&mut pos, &mut tris, 0.0, 3.0);
        // petit détail collé dehors (enseigne) : gardé
        cube(&mut pos, &mut tris, 10.4, 0.3);
        let m = outside_mask(&pos, &tris, [-10.0, -10.0, -10.0], [10.7, 10.7, 10.7]);
        assert!(m[..12].iter().filter(|v| **v).count() >= 10, "façade : {:?}", &m[..12]);
        assert!(m[12..24].iter().all(|v| !*v), "intérieur visible : {:?}", &m[12..24]);
        assert!(m[24..].iter().any(|v| *v));
    }

    #[test]
    fn big_model_is_fast_enough() {
        let (mut pos, mut tris) = (Vec::new(), Vec::new());
        for i in 0..1700 {
            cube(&mut pos, &mut tris, (i % 40) as f32 * 0.5 - 10.0, 0.2);
        }
        let t = std::time::Instant::now();
        let _ = outside_mask(&pos, &tris, [-10.5; 3], [10.5; 3]);
        assert!(t.elapsed().as_millis() < 3000, "{} ms pour {} triangles", t.elapsed().as_millis(), tris.len());
        println!("{} ms pour {} triangles", t.elapsed().as_millis(), tris.len());
    }
}
