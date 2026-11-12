//! Rendu de la vue : ciel, sol, objets en boîtes ombrées triées par profondeur,
//! intérieurs (pièces / portails), sélection et outils.

use crate::camera::Camera;
use crate::catalog::{self, Shape};
use crate::math::{v3, V3};
use crate::model::{Entity, ItemRef, Project};
use crate::sample;
use crate::gta::world::GameWorld;
use eframe::egui::{self, epaint::Vertex, pos2, Color32, Mesh, Painter, Pos2, Rect, Shape as EShape, Stroke};

pub struct Face {
    pts: Vec<V3>, // espace caméra, déjà découpé
    col: Color32,
    depth: f32,
}

/// Découpe un polygone (espace caméra) contre le plan proche.
fn clip_near(poly: &[V3], near: f32) -> Vec<V3> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let ain = a.z >= near;
        let bin = b.z >= near;
        if ain {
            out.push(a);
        }
        if ain != bin {
            let t = (near - a.z) / (b.z - a.z);
            out.push(a.lerp(b, t));
        }
    }
    out
}

fn fill_poly(mesh: &mut Mesh, cam: &Camera, cam_pts: &[V3], col: Color32) {
    if cam_pts.len() < 3 {
        return;
    }
    let base = mesh.vertices.len() as u32;
    for p in cam_pts {
        mesh.vertices.push(Vertex { pos: cam.cam_to_screen(*p), uv: egui::epaint::WHITE_UV, color: col });
    }
    for i in 1..(cam_pts.len() as u32 - 1) {
        mesh.indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }
}

fn shade(c: [u8; 3], n: V3, alpha: u8) -> Color32 {
    let light = v3(-0.45, -0.3, 0.85).norm();
    let k = 0.52 + 0.48 * n.dot(light).max(0.0);
    let s = |x: u8| ((x as f32) * k).min(255.0) as u8;
    Color32::from_rgba_unmultiplied(s(c[0]), s(c[1]), s(c[2]), alpha)
}

const BOX_FACES: [[usize; 4]; 6] = [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
pub const BOX_EDGES: [[usize; 2]; 12] = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];

fn box_corners(e: &Entity, mn: V3, mx: V3) -> [V3; 8] {
    let mut out = [V3::ZERO; 8];
    for (i, c) in out.iter_mut().enumerate() {
        let p = v3(
            if i & 1 == 0 { mn.x } else { mx.x },
            if i & 2 == 0 { mn.y } else { mx.y },
            if i & 4 == 0 { mn.z } else { mx.z },
        );
        *c = e.pos + e.rot.rotate(p);
    }
    out
}

fn push_box(faces: &mut Vec<Face>, cam: &Camera, w: &[V3; 8], col: [u8; 3], alpha: u8) {
    for f in BOX_FACES.iter() {
        let p0 = w[f[0]];
        let n = (w[f[1]] - p0).cross(w[f[2]] - p0).norm();
        let ctr = (p0 + w[f[2]]) * 0.5;
        let vis = if cam.map_view { n.dot(-cam.fwd) > 0.0 } else { n.dot(cam.eye - ctr) > 0.0 };
        if !vis {
            continue;
        }
        let cp: Vec<V3> = f.iter().map(|&i| cam.to_cam(w[i])).collect();
        let cp = clip_near(&cp, cam.near);
        if cp.len() < 3 {
            continue;
        }
        let depth = cp.iter().map(|p| p.z).sum::<f32>() / cp.len() as f32;
        faces.push(Face { pts: cp, col: shade(col, n, alpha), depth });
    }
}

/// Sous-boîtes (locales, mises à l'échelle) qui composent la forme d'un objet.
pub fn parts(e: &Entity, proj: &Project) -> Vec<(V3, V3, [u8; 3])> {
    let (mn, mx) = e.local_box(proj);
    let s = e.scale();
    let (mn, mx) = (mn.mulv(s), mx.mulv(s));
    let col = catalog::color_for(&e.archetype);
    let size = mx - mn;
    let c = (mn + mx) * 0.5;
    match catalog::shape_for(&e.archetype) {
        Shape::Box => vec![(mn, mx, col)],
        Shape::Tree => {
            let tw = (size.x.min(size.y) * 0.12).max(0.15);
            vec![
                (v3(c.x - tw, c.y - tw, mn.z), v3(c.x + tw, c.y + tw, mn.z + size.z * 0.45), [110, 80, 50]),
                (v3(mn.x, mn.y, mn.z + size.z * 0.3), v3(mx.x, mx.y, mx.z), col),
            ]
        }
        Shape::Pole => {
            let pw = (size.x.min(size.y) * 0.25).max(0.08);
            vec![
                (v3(c.x - pw, c.y - pw, mn.z), v3(c.x + pw, c.y + pw, mx.z), col),
                (v3(c.x - pw * 3.0, c.y - pw * 1.5, mx.z - 0.3), v3(c.x + pw * 3.0, c.y + pw * 1.5, mx.z), [235, 225, 170]),
            ]
        }
    }
}

/// Distance d'un rayon à la boîte d'un objet (dans le repère local).
pub fn ray_hit(e: &Entity, proj: &Project, o: V3, d: V3) -> Option<f32> {
    let (mn, mx) = e.local_box(proj);
    let s = e.scale();
    let (mn, mx) = (mn.mulv(s), mx.mulv(s));
    let inv = e.rot.inverse();
    let lo = inv.rotate(o - e.pos);
    let ld = inv.rotate(d);
    let mut t0 = f32::MIN;
    let mut t1 = f32::MAX;
    for (oo, dd, a, b) in [(lo.x, ld.x, mn.x, mx.x), (lo.y, ld.y, mn.y, mx.y), (lo.z, ld.z, mn.z, mx.z)] {
        if dd.abs() < 1e-9 {
            if oo < a || oo > b {
                return None;
            }
        } else {
            let mut ta = (a - oo) / dd;
            let mut tb = (b - oo) / dd;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
    }
    if t1 < 0.0 {
        None
    } else {
        Some(t0.max(0.0))
    }
}

pub struct ViewOpts {
    pub show_grid: bool,
    pub show_rooms: bool,
    pub show_sample: bool,
    /// compteur de performance en haut à gauche de la vue
    pub show_perf: bool,
}

/// Ce qu'il faut pour dessiner le monde du jeu.
pub struct GameCtx<'a> {
    pub world: &'a GameWorld,
    /// > 1 : moins de détails (plus rapide) ; < 1 : plus de détails
    pub lod_scale: f32,
    pub hover: Option<u32>,
    pub max_draw: usize,
    pub show_terrain: bool,
    /// vrais modèles 3D par la carte graphique (sinon des boîtes)
    pub gpu: Option<std::sync::Arc<std::sync::Mutex<crate::gpu::Gpu>>>,
    /// ombres du soleil
    pub shadows: bool,
    /// position de la souris (pour lire la profondeur sous le curseur)
    pub probe: Option<Pos2>,
    /// hauteur de coupe : rien n'est dessiné au-dessus (voir dans les bâtiments)
    pub cut_z: Option<f32>,
    /// objets du jeu cachés à la main (touche H)
    pub hidden: &'a std::collections::HashSet<u32>,
    /// dessiner les objets des intérieurs (Renderer.renderinteriors de CodeWalker)
    pub interiors: bool,
    /// objets du projet cachés à la main (touche H)
    pub hidden_ids: &'a std::collections::HashSet<u64>,
    /// carte vide : rien du jeu n'est dessiné
    pub empty_map: bool,
    /// groupes de LOD cachés (niveau, distance arrondie)
    pub hidden_lod: &'a std::collections::HashSet<(u8, u32)>,
    /// intérieur vu seul
    pub isolate: Option<u32>,
    /// objets du projet vus seuls (bouton « Isoler »)
    pub only: Option<&'a std::collections::HashSet<u64>>,
    /// terrain de l'éditeur : (modèle, origine, coins)
    pub terrain: Option<(u32, V3, V3, V3)>,
    /// zones d'eau du créateur de terrain (min x, min y, max x, max y, hauteur)
    pub waters: Vec<[f32; 5]>,
    /// objets en plus à dessiner : (modèle, position, orientation, échelle xy, échelle z)
    pub extra: Vec<(u32, V3, crate::math::Quat, f32, f32)>,
}

static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Direction du soleil (vers le soleil), la même que l'éclairage des faces.
pub const SUN_DIR: [f32; 3] = [-0.45, -0.3, 0.85];

/// Vue orthographique du soleil centrée sur `c`, demi-largeur `r` (pour la carte des ombres).
pub fn sun_view(c: V3, r: f32) -> [f32; 16] {
    let sun = v3(SUN_DIR[0], SUN_DIR[1], SUN_DIR[2]).norm();
    let eye = c + sun * (r * 3.0);
    let f = -sun;
    let right = f.cross(v3(0.0, 0.0, 1.0)).norm();
    let up = right.cross(f).norm();
    let depth = r * 6.0;
    let k = 2.0 / depth;
    let rows = [
        [right.x / r, right.y / r, right.z / r, -right.dot(eye) / r],
        [up.x / r, up.y / r, up.z / r, -up.dot(eye) / r],
        [f.x * k, f.y * k, f.z * k, -f.dot(eye) * k - 1.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut m = [0f32; 16];
    for (row, v) in rows.iter().enumerate() {
        for (col, x) in v.iter().enumerate() {
            m[col * 4 + row] = *x;
        }
    }
    m
}

/// Distance sous laquelle les objets d'un intérieur du jeu sont affichés (mètres) — choix de DragoWalker.
pub const MLO_SHOW_DIST: f32 = 250.0;

/// Hachage du nom d'un type d'objet (« hash_XXXXXXXX » accepté).
pub fn arch_hash(name: &str) -> u32 {
    if let Some(h) = name.strip_prefix("hash_") {
        if let Ok(v) = u32::from_str_radix(h, 16) {
            return v;
        }
    }
    crate::gta::crypto::joaat(name)
}

/// Ajoute les objets d'un intérieur du jeu placé en `pos` / `rot` (MloInstanceData::UpdateEntity 0x0600323d).
/// Un objet de groupe (variante) n'apparaît que si son groupe est dans `sets` : les fichiers du jeu donnent
/// le nom (hachage) du groupe, CodeWalker écrit sa position dans la liste (UpdateDefaultEntitySets 0x06003242).
#[allow(clippy::too_many_arguments)]
fn push_mlo_children(gl: &mut crate::gpu::Gpu, insts: &mut Vec<crate::gpu::Inst>, w: &GameWorld, def: &crate::gta::world::MloDef, pos: V3, rot: crate::math::Quat, sets: &[u32], eye: V3, lod_scale: f32, budget: &mut usize) {
    use crate::gpu::{model_matrix, Inst};
    for c in &def.children {
        if *budget == 0 {
            return;
        }
        if c.set >= 0 {
            let name = def.sets.get(c.set as usize).copied().unwrap_or(0);
            if !(sets.contains(&name) || sets.contains(&(c.set as u32))) {
                continue;
            }
        }
        // Renderer::RenderWorldAddInteriorEntities (0x0600098e) : tous les objets de l'intérieur sont dessinés,
        // sans distance propre à chacun ; seuls les « proxies » sont écartés (RenderIsEntityFinalRender
        // 0x06000991 : drapeau de type 2048, ou drapeaux d'objet 35127296, 39321602, 135790592/3, 536870912, 672661504)
        if matches!(c.flags, 35127296 | 39321602 | 135790592 | 135790593 | 536870912 | 672661504) || w.archetypes.get(&c.arch).map(|a| a.flags & 2048 != 0).unwrap_or(false) {
            continue;
        }
        let _ = (eye, lod_scale);
        let wp = pos + rot.rotate(c.pos);
        if w.hidden_archs.contains(&c.arch) {
            continue;
        }
        let wr = rot.mul(c.rot).normalized();
        gl.want(c.arch);
        let (mn, mx) = w.arch_box(c.arch);
        let col = w.color_of(c.arch);
        insts.push(Inst {
            arch: c.arch,
            m: model_matrix([wp.x, wp.y, wp.z], [wr.x, wr.y, wr.z, wr.w], [c.sxy, c.sxy, c.sz]),
            color: [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0],
            bmin: [mn.x, mn.y, mn.z],
            bmax: [mx.x, mx.y, mx.z],
        });
        *budget -= 1;
    }
}

/// Vrais modèles : liste des objets pour la carte graphique, dessinée par un rappel OpenGL.
fn push_gpu(painter: &Painter, cam: &Camera, proj: &Project, opts: &ViewOpts, g: &GameCtx, gpu: &std::sync::Arc<std::sync::Mutex<crate::gpu::Gpu>>, drawn: &mut Vec<u32>) -> Vec<[V3; 8]> {
    use crate::gpu::{model_matrix, FrameDraw, Inst};
    let w = g.world;
    let aspect = cam.rect.width() / cam.rect.height().max(1.0);
    let (eye, radius) = if cam.map_view {
        (v3(cam.target.x, cam.target.y, cam.target.z + cam.cur_ortho * 1.2), cam.cur_ortho * aspect.max(1.0) * 1.2 + 100.0)
    } else {
        (cam.eye, (cam.cur_dist * 4.0).clamp(1200.0, 6000.0))
    };
    let tan = (cam.fov * 0.5).tan();
    let mut cand: Vec<(f32, u32)> = Vec::new();
    let near_list = match g.isolate {
        Some(iso) => vec![iso],
        None if g.empty_map || g.only.is_some() => Vec::new(),
        None => w.visible_near(eye, cam.target, radius, g.lod_scale),
    };
    for ei in near_list {
        let e = &w.ents[ei as usize];
        if w.hidden_archs.contains(&e.arch) || g.hidden.contains(&ei) {
            continue;
        }
        if !g.hidden_lod.is_empty() && g.hidden_lod.contains(&(e.lod_level.min(5), crate::lists::lod_bucket(e.lod_dist()))) {
            continue;
        }
        let (mn, mx) = w.arch_box(e.arch);
        let rad = (mx - mn).len() * 0.5 * e.sxy().max(e.sz()) + 0.5;
        // entièrement au-dessus de la coupe : inutile de le dessiner
        if let Some(cz) = g.cut_z {
            if e.pos.z - rad > cz {
                continue;
            }
        }
        let ctr = e.pos + e.rot().rotate((mn + mx) * 0.5);
        let c = cam.to_cam(ctr);
        let size = if cam.map_view {
            if c.x.abs() > cam.cur_ortho * aspect + rad || c.y.abs() > cam.cur_ortho + rad {
                continue;
            }
            rad / cam.cur_ortho.max(1.0)
        } else {
            if c.z < -rad || c.x.abs() > c.z.max(0.0) * tan * aspect + rad * 1.5 || c.y.abs() > c.z.max(0.0) * tan + rad * 1.5 {
                continue;
            }
            rad / c.z.max(1.0)
        };
        if size < 0.0025 {
            continue;
        }
        cand.push((size, ei));
    }
    cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    cand.truncate(g.max_draw);
    let mut insts: Vec<Inst> = Vec::with_capacity(cand.len() + 64);
    let mut mlo_boxes = Vec::new();
    let Ok(mut gl) = gpu.lock() else { return mlo_boxes };
    let mut interior_budget = 4000usize;
    for (_, ei) in cand {
        let e = &w.ents[ei as usize];
        drawn.push(ei);
        if e.mlo {
            // Renderer::RenderWorld (0x06000988) : objets d'intérieur seulement si renderinteriors
            if !g.interiors {
                continue;
            }
            // intérieur du jeu : ses objets (meubles, murs intérieurs) quand on est assez près
            let def = w.mlo_defs.get(&e.arch);
            let near = (e.pos - eye).len() < MLO_SHOW_DIST * g.lod_scale.max(0.5);
            match def {
                Some(def) if near && interior_budget > 0 => {
                    let defaults = w.mlo_inst_sets.get(&ei).map(|v| v.as_slice()).unwrap_or(&[]);
                    push_mlo_children(&mut gl, &mut insts, w, def, e.pos, e.rot(), defaults, eye, g.lod_scale, &mut interior_budget);
                }
                Some(_) => {}
                None => mlo_boxes.push(w.corners(ei)),
            }
            continue;
        }
        gl.want(e.arch);
        let (mn, mx) = w.arch_box(e.arch);
        let col = w.color_of(e.arch);
        insts.push(Inst {
            arch: e.arch,
            m: model_matrix([e.pos.x, e.pos.y, e.pos.z], [e.rot().x, e.rot().y, e.rot().z, e.rot().w], [e.sxy(), e.sxy(), e.sz()]),
            color: [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0],
            bmin: [mn.x, mn.y, mn.z],
            bmax: [mx.x, mx.y, mx.z],
        });
    }
    // objets du projet (posés ou copiés du jeu) — pas pendant qu'un intérieur est vu seul
    let items = if g.isolate.is_some() { Vec::new() } else { proj.all_items() };
    for r in items {
        if matches!(r, ItemRef::Ent(y, _) if proj.ymaps[y].sample && !opts.show_sample) {
            continue;
        }
        let Some(e) = proj.world_entity(r) else { continue };
        if g.hidden_ids.contains(&e.id) || g.only.map(|o| !o.contains(&e.id)).unwrap_or(false) {
            continue;
        }
        if let Some(m) = &e.mlo {
            if !g.interiors {
                continue;
            }
            // intérieur du jeu copié dans le projet : ses objets, avec les variantes choisies dans le projet
            let h = arch_hash(&e.archetype);
            if let Some(def) = w.mlo_defs.get(&h) {
                if (e.pos - eye).len() < MLO_SHOW_DIST * g.lod_scale.max(0.5) {
                    let sets: Vec<u32> = m.default_entity_sets.iter().map(|s| arch_hash(s)).collect();
                    let rot = e.rot;
                    push_mlo_children(&mut gl, &mut insts, w, def, e.pos, rot, &sets, eye, g.lod_scale, &mut interior_budget);
                }
            }
            continue;
        }
        let c = cam.to_cam(e.pos);
        if !cam.map_view && (c.z < -50.0 || c.z > radius) {
            continue;
        }
        let h = arch_hash(&e.archetype);
        gl.want(h);
        let (mn, mx) = e.local_box(proj);
        let col = catalog::color_for(&e.archetype);
        insts.push(Inst {
            arch: h,
            m: model_matrix([e.pos.x, e.pos.y, e.pos.z], [e.rot.x, e.rot.y, e.rot.z, e.rot.w], [e.scale_xy, e.scale_xy, e.scale_z]),
            color: [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0],
            bmin: [mn.x, mn.y, mn.z],
            bmax: [mx.x, mx.y, mx.z],
        });
    }
    // objets en plus (aperçu de l'export d'un MLO : façades du jeu modifiées, là où elles seront posées)
    for (h, p, q, sxy, sz) in &g.extra {
        gl.want(*h);
        let (mn, mx) = w.arch_box(*h);
        insts.push(Inst {
            arch: *h,
            m: model_matrix([p.x, p.y, p.z], [q.x, q.y, q.z, q.w], [*sxy, *sxy, *sz]),
            color: [0.75, 0.7, 0.65],
            bmin: [mn.x, mn.y, mn.z],
            bmax: [mx.x, mx.y, mx.z],
        });
    }
    // terrain de l'éditeur
    if let (Some((arch, o, mn, mx)), None, None) = (g.terrain, g.isolate, g.only) {
        // libéré de la carte graphique : renvoyé depuis sa copie en mémoire (gpu.rs custom_src)
        gl.want(arch);
        insts.push(Inst {
            arch,
            m: model_matrix([o.x, o.y, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
            color: [0.6, 0.55, 0.45],
            bmin: [mn.x - o.x, mn.y - o.y, mn.z],
            bmax: [mx.x - o.x, mx.y - o.y, mx.z],
        });
    }
    drop(gl);
    let fd = std::sync::Arc::new(FrameDraw {
        vp: cam.view_proj(),
        eye: [eye.x, eye.y, eye.z],
        insts,
        fog: if cam.map_view { [0.6, 0.7, 0.8, 0.0] } else { [0.70, 0.78, 0.86, 0.00022] },
        sea: std::env::var("DW_NOSEA").is_err() && g.cut_z.is_none(),
        probe: g.probe.map(|p| [p.x, p.y]),
        light_vp: if cam.map_view || !g.shadows { None } else { Some(sun_view(cam.target, (cam.cur_dist * 1.2).clamp(60.0, 700.0))) },
        // carte vide : seulement une mer plate à 0 m autour du terrain
        water: if g.isolate.is_some() || g.only.is_some() {
            std::sync::Arc::new(Vec::new())
        } else {
            let mut v: Vec<[f32; 5]> = if g.empty_map { vec![[-9000.0, -9000.0, 9000.0, 9000.0, 0.0]] } else { w.water.as_ref().clone() };
            v.extend(g.waters.iter().copied());
            std::sync::Arc::new(v)
        },
        time: START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32(),
        clip_z: g.cut_z.unwrap_or(1.0e9),
    });
    let gpu2 = gpu.clone();
    painter.add(egui::PaintCallback {
        rect: cam.rect,
        callback: std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(move |info, p| {
            if let Ok(mut gg) = gpu2.lock() {
                unsafe { gg.paint(p.gl(), &info, &fd) };
            }
        })),
    });
    mlo_boxes
}

fn terrain_color(z: f32) -> [u8; 3] {
    if z < 1.5 {
        [196, 186, 146]
    } else if z < 12.0 {
        [186, 176, 136]
    } else if z < 180.0 {
        let t = ((z - 12.0) / 168.0).clamp(0.0, 1.0);
        [(104.0 + 30.0 * t) as u8, (134.0 - 6.0 * t) as u8, (84.0 + 10.0 * t) as u8]
    } else if z < 520.0 {
        [138, 124, 100]
    } else {
        [196, 194, 190]
    }
}

/// Relief du jeu autour de la caméra (quadrillage de la carte des hauteurs).
fn push_terrain(faces: &mut Vec<Face>, cam: &Camera, w: &GameWorld) {
    let aspect = cam.rect.width() / cam.rect.height().max(1.0);
    let (cx, cy, radius) = if cam.map_view {
        (cam.target.x, cam.target.y, cam.cur_ortho * aspect.max(1.0) * 1.15)
    } else {
        (cam.target.x, cam.target.y, (cam.cur_dist * 5.0).clamp(1500.0, 6000.0))
    };
    for hm in &w.heightmaps {
        if hm.w < 2 || hm.h < 2 {
            continue;
        }
        let cw = (hm.max.x - hm.min.x) / (hm.w - 1) as f32;
        let ch = (hm.max.y - hm.min.y) / (hm.h - 1) as f32;
        let x0 = ((cx - radius - hm.min.x) / cw).floor().max(0.0) as usize;
        let x1 = (((cx + radius - hm.min.x) / cw).ceil().max(0.0) as usize).min(hm.w - 1);
        let y0 = ((cy - radius - hm.min.y) / ch).floor().max(0.0) as usize;
        let y1 = (((cy + radius - hm.min.y) / ch).ceil().max(0.0) as usize).min(hm.h - 1);
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let cells = (x1 - x0) * (y1 - y0);
        let step = ((cells as f32 / 9000.0).sqrt().ceil() as usize).max(1);
        let z_at = |x: usize, y: usize| -> Option<f32> {
            let i = y * hm.w + x;
            if hm.max_h[i] == 0 && hm.min_h[i] == 0 {
                None
            } else {
                Some(hm.byte_to_z(hm.min_h[i]))
            }
        };
        let mut y = y0;
        while y + step <= y1 {
            let mut x = x0;
            while x + step <= x1 {
                let c = [(x, y), (x + step, y), (x + step, y + step), (x, y + step)];
                let zs: Vec<Option<f32>> = c.iter().map(|&(a, b)| z_at(a, b)).collect();
                if zs.iter().any(|z| z.is_some()) {
                    let pts: Vec<V3> = c
                        .iter()
                        .zip(zs.iter())
                        .map(|(&(a, b), z)| {
                            let (px, py) = hm.cell_pos(a, b);
                            v3(px, py, z.unwrap_or(0.0))
                        })
                        .collect();
                    let n = (pts[1] - pts[0]).cross(pts[3] - pts[0]).norm();
                    let avg = pts.iter().map(|p| p.z).sum::<f32>() / 4.0;
                    let cp: Vec<V3> = pts.iter().map(|p| cam.to_cam(*p)).collect();
                    let cp = clip_near(&cp, cam.near);
                    if cp.len() >= 3 {
                        let depth = cp.iter().map(|p| p.z).sum::<f32>() / cp.len() as f32 + if cam.map_view { 0.0 } else { 1.0 };
                        faces.push(Face { pts: cp, col: shade(terrain_color(avg), n, 255), depth });
                    }
                }
                x += step;
            }
            y += step;
        }
    }
}

/// Objets du jeu visibles : boîtes à la taille réelle de leur type, règle de niveau de détail du jeu.
fn push_game(faces: &mut Vec<Face>, painter: &Painter, cam: &Camera, g: &GameCtx, drawn: &mut Vec<u32>) {
    let w = g.world;
    let aspect = cam.rect.width() / cam.rect.height().max(1.0);
    let (eye, radius) = if cam.map_view {
        (v3(cam.target.x, cam.target.y, cam.target.z + cam.cur_ortho * 1.2), cam.cur_ortho * aspect.max(1.0) * 1.2 + 100.0)
    } else {
        (cam.eye, (cam.cur_dist * 4.0).clamp(800.0, 5000.0))
    };
    let tan = (cam.fov * 0.5).tan();
    let mut cand: Vec<(f32, u32)> = Vec::new();
    for ei in w.visible_near(eye, cam.target, radius, g.lod_scale) {
        let e = &w.ents[ei as usize];
        // les niveaux les plus grossiers (SLOD2, SLOD3, SLOD4) sont des blocs de quartier entiers :
        // le relief les remplace mieux en boîtes
        if matches!(e.lod_level, 3 | 4 | 6) || w.hidden_archs.contains(&e.arch) {
            continue;
        }
        let (mn, mx) = w.arch_box(e.arch);
        if (mx - mn).len() > 700.0 {
            continue;
        }
        let rad = (mx - mn).len() * 0.5 * e.sxy().max(e.sz()) + 0.5;
        let ctr = e.pos + e.rot().rotate((mn + mx) * 0.5);
        let c = cam.to_cam(ctr);
        let size = if cam.map_view {
            let half_w = cam.cur_ortho * aspect + rad;
            if c.x.abs() > half_w || c.y.abs() > cam.cur_ortho + rad {
                continue;
            }
            rad / cam.cur_ortho.max(1.0)
        } else {
            if c.z < -rad {
                continue;
            }
            if c.x.abs() > c.z.max(0.0) * tan * aspect + rad * 1.5 || c.y.abs() > c.z.max(0.0) * tan + rad * 1.5 {
                continue;
            }
            rad / c.z.max(1.0)
        };
        if size < 0.004 {
            continue;
        }
        cand.push((size, ei));
    }
    if cand.len() > g.max_draw {
        cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        cand.truncate(g.max_draw);
    }
    let mlo_stroke = Stroke::new(1.2_f32, Color32::from_rgb(80, 220, 255));
    for (_, ei) in cand {
        let e = &w.ents[ei as usize];
        let corners = w.corners(ei);
        drawn.push(ei);
        if e.mlo {
            draw_box_edges(painter, cam, &corners, mlo_stroke);
            continue;
        }
        let mut col = w.color_of(e.arch);
        if e.lod_level != 0 && e.lod_level != 5 {
            col = [(col[0] as u16 * 3 / 4 + 30) as u8, (col[1] as u16 * 3 / 4 + 36) as u8, (col[2] as u16 * 3 / 4 + 48) as u8];
        }
        push_box(faces, cam, &corners, col, 255);
    }
}

fn line3(painter: &Painter, cam: &Camera, a: V3, b: V3, stroke: Stroke) {
    let ca = cam.to_cam(a);
    let cb = cam.to_cam(b);
    let n = cam.near;
    if ca.z < n && cb.z < n {
        return;
    }
    let (ca, cb) = if ca.z < n {
        (ca.lerp(cb, (n - ca.z) / (cb.z - ca.z)), cb)
    } else if cb.z < n {
        (ca, ca.lerp(cb, (n - ca.z) / (cb.z - ca.z)))
    } else {
        (ca, cb)
    };
    painter.line_segment([cam.cam_to_screen(ca), cam.cam_to_screen(cb)], stroke);
}

pub fn draw_box_edges(painter: &Painter, cam: &Camera, w: &[V3; 8], stroke: Stroke) {
    for e in BOX_EDGES.iter() {
        line3(painter, cam, w[e[0]], w[e[1]], stroke);
    }
}

pub fn draw_ground_circle(painter: &Painter, cam: &Camera, c: V3, r: f32, stroke: Stroke) {
    let n = 48;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        line3(painter, cam, c + v3(a0.cos() * r, a0.sin() * r, 0.2), c + v3(a1.cos() * r, a1.sin() * r, 0.2), stroke);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_scene(
    painter: &Painter,
    cam: &Camera,
    proj: &Project,
    opts: &ViewOpts,
    selected: &std::collections::HashSet<u64>,
    hover: Option<u64>,
    cur_mlo: Option<(usize, usize)>,
    game: Option<&GameCtx>,
    drawn: &mut Vec<u32>,
) {
    drawn.clear();
    let rect = cam.rect;
    // ciel (dégradé)
    if cam.map_view {
        painter.rect_filled(rect, 0.0, Color32::from_rgb(38, 84, 120));
    } else {
        let mut sky = Mesh::default();
        let top = Color32::from_rgb(78, 128, 190);
        let hor = Color32::from_rgb(196, 214, 228);
        let idx = sky.vertices.len() as u32;
        for (p, c) in [(rect.left_top(), top), (rect.right_top(), top), (rect.right_bottom(), hor), (rect.left_bottom(), hor)] {
            sky.vertices.push(Vertex { pos: p, uv: egui::epaint::WHITE_UV, color: c });
        }
        sky.indices.extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
        painter.add(EShape::mesh(sky));
    }

    // sol
    let mut ground = Mesh::default();
    if opts.show_sample {
        for (q, col) in sample::ground() {
            let cp: Vec<V3> = q.iter().map(|p| cam.to_cam(*p)).collect();
            let cp = clip_near(&cp, cam.near);
            let c = if cam.map_view { col } else { [col[0], col[1], col[2]] };
            fill_poly(&mut ground, cam, &cp, Color32::from_rgb(c[0], c[1], c[2]));
        }
    } else if game.map(|g| g.gpu.is_some()).unwrap_or(false) {
        // la mer est dessinée par la carte graphique
    } else if game.is_some() {
        // la mer (z = 0) ; le relief est dessiné avec les objets
        let w = 20000.0;
        let q = [v3(-w, -w, 0.0), v3(w, -w, 0.0), v3(w, w, 0.0), v3(-w, w, 0.0)];
        let cp: Vec<V3> = q.iter().map(|p| cam.to_cam(*p)).collect();
        fill_poly(&mut ground, cam, &clip_near(&cp, cam.near), Color32::from_rgb(40, 88, 128));
    } else {
        let w = 5000.0;
        let q = [v3(-w, -w, 0.0), v3(w, -w, 0.0), v3(w, w, 0.0), v3(-w, w, 0.0)];
        let cp: Vec<V3> = q.iter().map(|p| cam.to_cam(*p)).collect();
        fill_poly(&mut ground, cam, &clip_near(&cp, cam.near), Color32::from_rgb(95, 100, 92));
    }
    painter.add(EShape::mesh(ground));

    // grille (10 m près de la cible)
    if opts.show_grid {
        let step = if cam.map_view { (cam.cur_ortho / 10.0).max(10.0) } else { (cam.cur_dist / 6.0).max(10.0) };
        let step = 10f32.powf(step.log10().round());
        let n = 20;
        let cx = (cam.target.x / step).round() * step;
        let cy = (cam.target.y / step).round() * step;
        let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 28));
        for i in -n..=n {
            let o = i as f32 * step;
            line3(painter, cam, v3(cx + o, cy - n as f32 * step, 0.05), v3(cx + o, cy + n as f32 * step, 0.05), stroke);
            line3(painter, cam, v3(cx - n as f32 * step, cy + o, 0.05), v3(cx + n as f32 * step, cy + o, 0.05), stroke);
        }
    }

    // objets
    let mut faces: Vec<Face> = Vec::with_capacity(8000);
    let tan = (cam.fov * 0.5).tan();
    let aspect = rect.width() / rect.height().max(1.0);
    let far = if cam.map_view { f32::MAX } else { 6000.0 };
    let mut drawn_boxes: Vec<(u64, [V3; 8])> = Vec::new();
    let use_gpu = game.map(|g| g.gpu.is_some()).unwrap_or(false);
    let mut gpu_mlo: Vec<[V3; 8]> = Vec::new();
    if let Some(g) = game {
        if let Some(gpu) = &g.gpu {
            gpu_mlo = push_gpu(painter, cam, proj, opts, g, gpu, drawn);
        } else {
            if g.show_terrain {
                push_terrain(&mut faces, cam, g.world);
            }
            push_game(&mut faces, painter, cam, g, drawn);
        }
    }
    for r in proj.all_items() {
        let is_sample = matches!(r, ItemRef::Ent(y, _) if proj.ymaps[y].sample);
        if is_sample && !opts.show_sample {
            continue;
        }
        let Some(e) = proj.world_entity(r) else { continue };
        if game.map(|g| g.hidden_ids.contains(&e.id)).unwrap_or(false) {
            continue;
        }
        let (mn, mx) = e.local_box(proj);
        let rad = ((mx - mn).mulv(e.scale())).len() * 0.5 + 1.0;
        let c = cam.to_cam(e.pos);
        if !cam.map_view {
            if c.z < -rad || c.z > far {
                continue;
            }
            if c.x.abs() > c.z.max(0.0) * tan * aspect + rad * 2.0 || c.y.abs() > c.z.max(0.0) * tan + rad * 2.0 + 50.0 {
                continue;
            }
            // les petits objets lointains ne sont pas dessinés
            if rad < 3.0 && c.z > 900.0 {
                continue;
            }
        } else {
            let half_w = cam.cur_ortho * aspect + rad;
            if c.x.abs() > half_w || c.y.abs() > cam.cur_ortho + rad {
                continue;
            }
        }
        let alpha = 255;
        if use_gpu {
            if selected.contains(&e.id) || hover == Some(e.id) {
                drawn_boxes.push((e.id, e.corners(proj)));
            }
            continue;
        }
        for (pmn, pmx, col) in parts(&e, proj) {
            let mut ee = e.clone();
            ee.scale_xy = 1.0;
            ee.scale_z = 1.0;
            let w = box_corners(&ee, pmn, pmx);
            push_box(&mut faces, cam, &w, col, alpha);
        }
        if selected.contains(&e.id) || hover == Some(e.id) {
            drawn_boxes.push((e.id, e.corners(proj)));
        }
    }
    faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    let mut mesh = Mesh::default();
    let outline = Stroke::new(0.6_f32, Color32::from_rgba_unmultiplied(20, 20, 25, 70));
    for f in &faces {
        fill_poly(&mut mesh, cam, &f.pts, f.col);
        if mesh.vertices.len() > 60000 {
            painter.add(EShape::mesh(std::mem::take(&mut mesh)));
        }
    }
    painter.add(EShape::mesh(mesh));
    let _ = outline;

    // intérieurs MLO : pièces et portails
    if opts.show_rooms {
        for (ti, t) in proj.ytyps.iter().enumerate() {
            if !t.visible {
                continue;
            }
            for (mi, m) in t.mlos.iter().enumerate() {
                let (p, q) = proj.mlo_placement(&m.name).unwrap_or((V3::ZERO, crate::math::Quat::IDENTITY));
                let current = cur_mlo == Some((ti, mi));
                let room_stroke = Stroke::new(if current { 2.0 } else { 1.2_f32 }, Color32::from_rgb(80, 220, 255));
                let mut floor = Mesh::default();
                for (ri, r) in m.rooms.iter().enumerate().skip(1) {
                    let mut w = [V3::ZERO; 8];
                    for (i, c) in w.iter_mut().enumerate() {
                        let l = v3(
                            if i & 1 == 0 { r.bb_min.x } else { r.bb_max.x },
                            if i & 2 == 0 { r.bb_min.y } else { r.bb_max.y },
                            if i & 4 == 0 { r.bb_min.z } else { r.bb_max.z },
                        );
                        *c = p + q.rotate(l);
                    }
                    let fl: Vec<V3> = [0, 1, 3, 2].iter().map(|&i| cam.to_cam(w[i] + v3(0.0, 0.0, 0.03))).collect();
                    let tint = if ri % 2 == 0 { Color32::from_rgba_unmultiplied(80, 200, 255, 60) } else { Color32::from_rgba_unmultiplied(120, 170, 255, 60) };
                    fill_poly(&mut floor, cam, &clip_near(&fl, cam.near), tint);
                    draw_box_edges(painter, cam, &w, room_stroke);
                    if let Some((sp, _)) = cam.project(p + q.rotate((r.bb_min + r.bb_max) * 0.5 + v3(0.0, 0.0, (r.bb_max.z - r.bb_min.z) * 0.5))) {
                        if rect.contains(sp) && (cam.map_view || cam.cur_dist < 400.0) {
                            painter.text(sp, egui::Align2::CENTER_CENTER, &r.name, egui::FontId::proportional(12.0), Color32::from_rgb(200, 245, 255));
                        }
                    }
                }
                painter.add(EShape::mesh(floor));
                for pt in &m.portals {
                    let wc: Vec<V3> = pt.corners.iter().map(|c| p + q.rotate(*c)).collect();
                    let cp: Vec<V3> = wc.iter().map(|c| cam.to_cam(*c)).collect();
                    let mut pm = Mesh::default();
                    let col = if pt.room_to == 0 || pt.room_from == 0 { Color32::from_rgba_unmultiplied(255, 170, 40, 120) } else { Color32::from_rgba_unmultiplied(255, 90, 200, 110) };
                    fill_poly(&mut pm, cam, &clip_near(&cp, cam.near), col);
                    painter.add(EShape::mesh(pm));
                    for i in 0..4 {
                        line3(painter, cam, wc[i], wc[(i + 1) % 4], Stroke::new(1.5_f32, col.to_opaque()));
                    }
                }
            }
        }
    }

    // intérieurs du jeu (vrais modèles) : leur volume en fil
    for w in &gpu_mlo {
        draw_box_edges(painter, cam, w, Stroke::new(1.2_f32, Color32::from_rgb(80, 220, 255)));
    }
    // objet du jeu survolé
    if let Some(g) = game {
        if let Some(h) = g.hover {
            if (h as usize) < g.world.ents.len() {
                draw_box_edges(painter, cam, &g.world.corners(h), Stroke::new(1.5_f32, Color32::WHITE));
            }
        }
    }
    // sélection et survol
    for (id, w) in &drawn_boxes {
        let stroke = if selected.contains(id) { Stroke::new(2.0_f32, Color32::from_rgb(255, 214, 40)) } else { Stroke::new(1.5_f32, Color32::WHITE) };
        draw_box_edges(painter, cam, w, stroke);
    }
}

/// Dessine un cadre de prévisualisation (prop à poser).
pub fn draw_ghost(painter: &Painter, cam: &Camera, proj: &Project, e: &Entity) {
    let w = e.corners(proj);
    draw_box_edges(painter, cam, &w, Stroke::new(1.5_f32, Color32::from_rgb(120, 255, 140)));
}

#[allow(dead_code)]
pub fn screen_rect_contains(r: Rect, p: Pos2) -> bool {
    r.contains(p)
}

pub fn _unused() -> Pos2 {
    pos2(0.0, 0.0)
}
