//! Studio — props : créer ses propres objets FiveM sans quitter DragoWalker.
//!
//! Trois façons : une image (panneau, affiche, découpe, tapis, cube), un modèle 3D (.glb / .gltf / .obj lus
//! directement, .blend / .fbx / .dae / .stl passés par Blender en arrière-plan), ou des formes simples assemblées
//! (boîtes, cylindres, plaques) avec couleurs ou textures importées. Chaque prop est écrit au format XML de
//! CodeWalker dans le dossier d'échange des modèles (« Mes modèles ») : modèle (.ydr.xml, collision intégrée),
//! dictionnaire de textures (.ytd.xml) et type d'objet (.ytyp.xml). Il apparaît tout seul dans « Mes modèles »,
//! se pose sur la carte, part avec l'export de la ressource, ou s'exporte seul en ressource FiveM.

use crate::app::DwApp;
use crate::gta::drawable::{RGeom, RModel, RVert, TexData};
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Drapeaux du type d'objet : ceux de prop_bench_01a (prop fixe du jeu à collision intégrée), relevés par la
/// sonde DW_ARCH_FLAGS (16 nov. 2026) — **estimé** pour un prop personnel.
pub const PROP_FLAGS: u32 = 0x2002_0000;
/// Côté maximal des textures d'un prop (px).
const MAX_TEX: u32 = 1024;
/// Au-delà, la collision « forme exacte » devient une boîte (triangles) — **estimé**.
const MAX_COLL_TRIS: usize = 20000;

/// Partie d'un maillage : triangles, coordonnées de texture, image (rang dans `Mesh3::images`) ou couleur.
#[derive(Clone, Default)]
pub struct Part {
    pub pos: Vec<V3>,
    pub nrm: Vec<V3>,
    pub uv: Vec<[f32; 2]>,
    pub idx: Vec<u32>,
    pub image: Option<usize>,
    pub color: [u8; 4],
    pub cutout: bool,
}

/// Maillage importé (repère du jeu : z vers le haut, mètres).
#[derive(Clone, Default)]
pub struct Mesh3 {
    pub parts: Vec<Part>,
    pub images: Vec<image::RgbaImage>,
}

impl Mesh3 {
    pub fn tris(&self) -> usize {
        self.parts.iter().map(|p| p.idx.len() / 3).sum()
    }

    pub fn bounds(&self) -> (V3, V3) {
        let (mut a, mut b) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
        for p in self.parts.iter().flat_map(|p| p.pos.iter()) {
            a = a.min(*p);
            b = b.max(*p);
        }
        if a.x > b.x {
            return (V3::ZERO, V3::ZERO);
        }
        (a, b)
    }

    /// Met à l'échelle et, avec `ground`, pose le bas au centre sur l'origine (le prop se pose au sol).
    pub fn fit(&mut self, scale: f32, ground: bool) {
        for p in self.parts.iter_mut() {
            for q in p.pos.iter_mut() {
                *q = *q * scale;
            }
        }
        if ground {
            let (a, b) = self.bounds();
            let off = v3((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, a.z);
            for p in self.parts.iter_mut() {
                for q in p.pos.iter_mut() {
                    *q = *q - off;
                }
            }
        }
    }
}

/// Normales calculées depuis les triangles (quand le fichier n'en a pas).
fn smooth_normals(pos: &[V3], idx: &[u32]) -> Vec<V3> {
    let mut n = vec![V3::ZERO; pos.len()];
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let f = (b - a).cross(c - a);
        for k in t {
            n[*k as usize] = n[*k as usize] + f;
        }
    }
    n.into_iter().map(|x| if x.len() > 1e-9 { x.norm() } else { v3(0.0, 0.0, 1.0) }).collect()
}

/// Repère glTF / OBJ de Blender (y vers le haut, -z devant) → repère du jeu (z vers le haut) : rotation, le sens
/// des triangles est gardé.
fn yup(p: [f32; 3]) -> V3 {
    v3(p[0], -p[2], p[1])
}

// ---------------------------------------------------------------------------------------------------------------
// glTF 2.0 (.glb binaire ou .gltf texte)

type M4 = [f32; 16];
const I4: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mul4(a: &M4, b: &M4) -> M4 {
    // colonnes d'abord (comme glTF)
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> M4 {
    let [x, y, z, w] = q;
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [r[0] * s[0], r[1] * s[0], r[2] * s[0], 0.0, r[3] * s[1], r[4] * s[1], r[5] * s[1], 0.0, r[6] * s[2], r[7] * s[2], r[8] * s[2], 0.0, t[0], t[1], t[2], 1.0]
}

fn xf_point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]]
}

fn xf_dir(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [m[0] * p[0] + m[4] * p[1] + m[8] * p[2], m[1] * p[0] + m[5] * p[1] + m[9] * p[2], m[2] * p[0] + m[6] * p[1] + m[10] * p[2]]
}

fn det3(m: &M4) -> f32 {
    m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2])
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for ch in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in ch.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if ch.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if ch.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Lit un fichier glTF 2.0 (.glb ou .gltf) : maillages de la scène avec leurs transformations, textures de couleur
/// (PNG / JPEG intégrées ou à côté), couleur de base, découpe (alphaMode MASK / BLEND).
pub fn read_gltf(path: &Path) -> Result<Mesh3, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or(Path::new("."));
    let (json, bin): (serde_json::Value, Option<Vec<u8>>) = if data.len() >= 12 && &data[0..4] == b"glTF" {
        let mut off = 12usize;
        let (mut js, mut bin) = (None, None);
        while off + 8 <= data.len() {
            let len = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as usize;
            let ty = &data[off + 4..off + 8];
            let body = data.get(off + 8..off + 8 + len).ok_or("fichier .glb tronqué")?;
            if ty == b"JSON" {
                js = Some(serde_json::from_slice(body).map_err(|e| format!("glTF illisible : {e}"))?);
            } else if ty == b"BIN\0" {
                bin = Some(body.to_vec());
            }
            off += 8 + len;
        }
        (js.ok_or("glTF sans partie JSON")?, bin)
    } else {
        (serde_json::from_slice(&data).map_err(|e| format!("glTF illisible : {e}"))?, None)
    };
    let arr = |v: &serde_json::Value, k: &str| v[k].as_array().cloned().unwrap_or_default();
    // tampons
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for (i, b) in arr(&json, "buffers").iter().enumerate() {
        match b["uri"].as_str() {
            Some(u) if u.starts_with("data:") => buffers.push(u.split_once(",").and_then(|x| b64_decode(x.1)).ok_or("tampon intégré illisible")?),
            Some(u) => buffers.push(std::fs::read(base.join(u.replace("%20", " "))).map_err(|e| format!("tampon {u} : {e}"))?),
            None if i == 0 => buffers.push(bin.clone().ok_or("tampon binaire absent")?),
            None => buffers.push(Vec::new()),
        }
    }
    let views = arr(&json, "bufferViews");
    let accessors = arr(&json, "accessors");
    let view_bytes = |vi: usize| -> Option<(&[u8], usize)> {
        let v = views.get(vi)?;
        let b = buffers.get(v["buffer"].as_u64()? as usize)?;
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = v["byteLength"].as_u64()? as usize;
        Some((b.get(off..off + len)?, v["byteStride"].as_u64().unwrap_or(0) as usize))
    };
    // valeurs d'un accesseur, en f32 (composantes à la suite)
    let read_acc = |ai: usize| -> Option<(Vec<f32>, usize)> {
        let a = accessors.get(ai)?;
        let n = a["count"].as_u64()? as usize;
        let comps = match a["type"].as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let ct = a["componentType"].as_u64()?;
        let norm = a["normalized"].as_bool().unwrap_or(false);
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        let Some((bytes, stride)) = a["bufferView"].as_u64().and_then(|v| view_bytes(v as usize)) else {
            return Some((vec![0.0; n * comps], comps));
        };
        let stride = if stride == 0 { csize * comps } else { stride };
        let start = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let mut out = Vec::with_capacity(n * comps);
        for i in 0..n {
            for c in 0..comps {
                let o = start + i * stride + c * csize;
                let b = bytes.get(o..o + csize)?;
                let v = match ct {
                    5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                    5123 => {
                        let x = u16::from_le_bytes([b[0], b[1]]) as f32;
                        if norm {
                            x / 65535.0
                        } else {
                            x
                        }
                    }
                    5122 => {
                        let x = i16::from_le_bytes([b[0], b[1]]) as f32;
                        if norm {
                            (x / 32767.0).max(-1.0)
                        } else {
                            x
                        }
                    }
                    5121 => {
                        let x = b[0] as f32;
                        if norm {
                            x / 255.0
                        } else {
                            x
                        }
                    }
                    _ => {
                        let x = b[0] as i8 as f32;
                        if norm {
                            (x / 127.0).max(-1.0)
                        } else {
                            x
                        }
                    }
                };
                out.push(v);
            }
        }
        Some((out, comps))
    };
    // images (décodées une fois)
    let mut images: Vec<Option<image::RgbaImage>> = Vec::new();
    for im in arr(&json, "images") {
        let bytes = if let Some(v) = im["bufferView"].as_u64() {
            view_bytes(v as usize).map(|x| x.0.to_vec())
        } else if let Some(u) = im["uri"].as_str() {
            if u.starts_with("data:") {
                u.split_once(",").and_then(|x| b64_decode(x.1))
            } else {
                std::fs::read(base.join(u.replace("%20", " "))).ok()
            }
        } else {
            None
        };
        images.push(bytes.and_then(|b| image::load_from_memory(&b).ok()).map(|i| i.to_rgba8()));
    }
    let textures = arr(&json, "textures");
    let materials = arr(&json, "materials");
    let meshes = arr(&json, "meshes");
    let nodes = arr(&json, "nodes");
    // nœuds de la scène, avec leur transformation dans le monde
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = json["scenes"].get(scene).map(|s| arr(s, "nodes").iter().filter_map(|n| n.as_u64().map(|x| x as usize)).collect()).unwrap_or_else(|| (0..nodes.len()).collect());
    let mut stack: Vec<(usize, M4, u32)> = roots.into_iter().map(|n| (n, I4, 0)).collect();
    let mut out = Mesh3::default();
    let mut img_map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    while let Some((ni, parent, depth)) = stack.pop() {
        let Some(node) = nodes.get(ni) else { continue };
        if depth > 64 {
            continue;
        }
        let local = if let Some(m) = node["matrix"].as_array().filter(|m| m.len() == 16) {
            let mut r = [0.0; 16];
            for (i, v) in m.iter().enumerate() {
                r[i] = v.as_f64().unwrap_or(0.0) as f32;
            }
            r
        } else {
            let f = |k: &str, d: &[f32]| -> Vec<f32> { node[k].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_else(|| d.to_vec()) };
            let (t, q, s) = (f("translation", &[0.0; 3]), f("rotation", &[0.0, 0.0, 0.0, 1.0]), f("scale", &[1.0; 3]));
            trs([t[0], t[1], t[2]], [q[0], q[1], q[2], q[3]], [s[0], s[1], s[2]])
        };
        let world = mul4(&parent, &local);
        for c in arr(node, "children") {
            if let Some(c) = c.as_u64() {
                stack.push((c as usize, world, depth + 1));
            }
        }
        let Some(mesh) = node["mesh"].as_u64().and_then(|m| meshes.get(m as usize)) else { continue };
        let flip = det3(&world) < 0.0;
        for prim in arr(mesh, "primitives") {
            if prim["mode"].as_u64().unwrap_or(4) != 4 {
                continue; // seulement des triangles
            }
            let at = &prim["attributes"];
            let Some((p, 3)) = at["POSITION"].as_u64().and_then(|a| read_acc(a as usize)) else { continue };
            let n = p.len() / 3;
            let pos: Vec<V3> = p.chunks_exact(3).map(|c| yup(xf_point(&world, [c[0], c[1], c[2]]))).collect();
            let uv: Vec<[f32; 2]> = at["TEXCOORD_0"].as_u64().and_then(|a| read_acc(a as usize)).filter(|x| x.1 == 2).map(|x| x.0.chunks_exact(2).map(|c| [c[0], c[1]]).collect()).unwrap_or_else(|| vec![[0.0, 0.0]; n]);
            let mut idx: Vec<u32> = match prim["indices"].as_u64().and_then(|a| read_acc(a as usize)) {
                Some((v, 1)) => v.into_iter().map(|x| x as u32).filter(|i| (*i as usize) < n).collect(),
                _ => (0..n as u32).collect(),
            };
            idx.truncate(idx.len() / 3 * 3);
            if flip {
                for t in idx.chunks_exact_mut(3) {
                    t.swap(1, 2);
                }
            }
            let nrm: Vec<V3> = match at["NORMAL"].as_u64().and_then(|a| read_acc(a as usize)) {
                Some((v, 3)) if v.len() == p.len() => v.chunks_exact(3).map(|c| yup(xf_dir(&world, [c[0], c[1], c[2]]))).map(|x| if x.len() > 1e-9 { x.norm() } else { v3(0.0, 0.0, 1.0) }).collect(),
                _ => smooth_normals(&pos, &idx),
            };
            let mat = prim["material"].as_u64().and_then(|m| materials.get(m as usize));
            let mut color = [255u8; 4];
            let mut image = None;
            let mut cutout = false;
            if let Some(m) = mat {
                let pbr = &m["pbrMetallicRoughness"];
                if let Some(f) = pbr["baseColorFactor"].as_array() {
                    for (k, v) in f.iter().take(4).enumerate() {
                        color[k] = (v.as_f64().unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
                if let Some(src) = pbr["baseColorTexture"]["index"].as_u64().and_then(|t| textures.get(t as usize)).and_then(|t| t["source"].as_u64()) {
                    let src = src as usize;
                    if let Some(Some(img)) = images.get(src) {
                        let k = *img_map.entry(src).or_insert_with(|| {
                            out.images.push(img.clone());
                            out.images.len() - 1
                        });
                        image = Some(k);
                    }
                }
                cutout = matches!(m["alphaMode"].as_str(), Some("MASK") | Some("BLEND"));
            }
            out.parts.push(Part { pos, nrm, uv, idx, image, color, cutout });
        }
    }
    if out.parts.is_empty() {
        return Err("aucun maillage (triangles) dans ce fichier".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// OBJ (+ .mtl)

/// Lit un fichier .obj (et son .mtl : couleur Kd, texture map_Kd, transparence d) ; polygones coupés en triangles.
pub fn read_obj(path: &Path) -> Result<Mesh3, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or(Path::new("."));
    let (mut vp, mut vt, mut vn): (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 3]>) = (Vec::new(), Vec::new(), Vec::new());
    // matériaux : couleur, image, découpe
    let mut mats: std::collections::HashMap<String, ([u8; 4], Option<PathBuf>)> = std::collections::HashMap::new();
    let mut cur_mat = String::new();
    // une partie par matériau : sommets (position, uv, normale) dédoublonnés
    struct Build {
        part: Part,
        seen: std::collections::HashMap<(i64, i64, i64), u32>,
        has_n: bool,
    }
    let mut builds: Vec<(String, Build)> = Vec::new();
    let f32s = |it: &mut std::str::SplitWhitespace| -> Vec<f32> { it.filter_map(|x| x.parse::<f32>().ok()).collect() };
    for line in src.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v = f32s(&mut it);
                if v.len() >= 3 {
                    vp.push([v[0], v[1], v[2]]);
                }
            }
            Some("vt") => {
                let v = f32s(&mut it);
                if v.len() >= 2 {
                    vt.push([v[0], 1.0 - v[1]]);
                }
            }
            Some("vn") => {
                let v = f32s(&mut it);
                if v.len() >= 3 {
                    vn.push([v[0], v[1], v[2]]);
                }
            }
            Some("mtllib") => {
                let file = line.trim_start()[6..].trim();
                if let Ok(m) = std::fs::read_to_string(base.join(file)) {
                    let mut name = String::new();
                    for l in m.lines() {
                        let mut w = l.split_whitespace();
                        match w.next() {
                            Some("newmtl") => {
                                name = w.collect::<Vec<_>>().join(" ");
                                mats.insert(name.clone(), ([255; 4], None));
                            }
                            Some("Kd") => {
                                let v = f32s(&mut w);
                                if let (Some(e), 3..) = (mats.get_mut(&name), v.len()) {
                                    for k in 0..3 {
                                        e.0[k] = (v[k].clamp(0.0, 1.0) * 255.0) as u8;
                                    }
                                }
                            }
                            Some("d") => {
                                let v = f32s(&mut w);
                                if let (Some(e), Some(d)) = (mats.get_mut(&name), v.first()) {
                                    e.0[3] = (d.clamp(0.0, 1.0) * 255.0) as u8;
                                }
                            }
                            Some("map_Kd") => {
                                // dernier mot : le fichier (les options -s, -o… viennent avant)
                                let rest: Vec<&str> = w.collect();
                                if let (Some(e), Some(f)) = (mats.get_mut(&name), rest.last()) {
                                    e.1 = Some(base.join(f.replace('\\', "/")));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("usemtl") => cur_mat = it.collect::<Vec<_>>().join(" "),
            Some("f") => {
                let bi = match builds.iter().position(|b| b.0 == cur_mat) {
                    Some(i) => i,
                    None => {
                        builds.push((cur_mat.clone(), Build { part: Part::default(), seen: Default::default(), has_n: true }));
                        builds.len() - 1
                    }
                };
                let b = &mut builds[bi].1;
                let mut poly = Vec::new();
                for w in it {
                    let mut s = w.split('/');
                    let r = |x: Option<&str>, n: usize| -> i64 {
                        match x.and_then(|v| v.parse::<i64>().ok()) {
                            Some(i) if i < 0 => n as i64 + i,
                            Some(i) => i - 1,
                            None => -1,
                        }
                    };
                    let (pi, ti, ni) = (r(s.next(), vp.len()), r(s.next(), vt.len()), r(s.next(), vn.len()));
                    if pi < 0 || pi as usize >= vp.len() {
                        continue;
                    }
                    let key = (pi, ti, ni);
                    let id = match b.seen.get(&key) {
                        Some(i) => *i,
                        None => {
                            let i = b.part.pos.len() as u32;
                            b.part.pos.push(yup(vp[pi as usize]));
                            b.part.uv.push(if ti >= 0 && (ti as usize) < vt.len() { vt[ti as usize] } else { [0.0, 0.0] });
                            if ni >= 0 && (ni as usize) < vn.len() {
                                b.part.nrm.push(yup(vn[ni as usize]));
                            } else {
                                b.part.nrm.push(V3::ZERO);
                                b.has_n = false;
                            }
                            b.seen.insert(key, i);
                            i
                        }
                    };
                    poly.push(id);
                }
                for k in 1..poly.len().saturating_sub(1) {
                    b.part.idx.extend_from_slice(&[poly[0], poly[k], poly[k + 1]]);
                }
            }
            _ => {}
        }
    }
    let mut out = Mesh3::default();
    let mut img_of: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    for (name, mut b) in builds {
        if b.part.idx.is_empty() {
            continue;
        }
        if !b.has_n {
            b.part.nrm = smooth_normals(&b.part.pos, &b.part.idx);
        }
        if let Some((c, tex)) = mats.get(&name) {
            b.part.color = *c;
            b.part.cutout = c[3] < 250;
            if let Some(t) = tex {
                if let Some(k) = img_of.get(t) {
                    b.part.image = Some(*k);
                } else if let Some(img) = std::fs::read(t).ok().and_then(|d| image::load_from_memory(&d).ok()) {
                    out.images.push(img.to_rgba8());
                    img_of.insert(t.clone(), out.images.len() - 1);
                    b.part.image = Some(out.images.len() - 1);
                }
            }
        } else {
            b.part.color = [200, 200, 200, 255];
        }
        out.parts.push(b.part);
    }
    if out.parts.is_empty() {
        return Err("aucune face dans ce fichier .obj".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// Blender en arrière-plan

/// Script Blender : ouvre / importe le fichier source et l'exporte en .glb (modificateurs appliqués).
const BLENDER_SCRIPT: &str = r#"
import bpy, sys
argv = sys.argv[sys.argv.index("--") + 1:]
src, dst = argv[0], argv[1]
ext = src.lower().rsplit(".", 1)[-1]
if ext != "blend":
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if ext == "fbx":
        bpy.ops.import_scene.fbx(filepath=src)
    elif ext == "obj":
        bpy.ops.wm.obj_import(filepath=src)
    elif ext == "dae":
        bpy.ops.wm.collada_import(filepath=src)
    elif ext == "stl":
        bpy.ops.wm.stl_import(filepath=src)
    elif ext == "ply":
        bpy.ops.wm.ply_import(filepath=src)
    elif ext in ("glb", "gltf"):
        bpy.ops.import_scene.gltf(filepath=src)
bpy.ops.export_scene.gltf(filepath=dst, export_format="GLB", export_apply=True)
"#;

/// Convertit un fichier 3D en .glb avec Blender (sans fenêtre). Rend le chemin du .glb.
pub fn blender_to_glb(blender: &Path, src: &Path) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("dragowalker_props");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("dw_to_glb.py");
    std::fs::write(&script, BLENDER_SCRIPT).map_err(|e| e.to_string())?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "modele".into());
    let dst = dir.join(format!("{stem}_{}.glb", crate::cwbridge::unique()));
    let mut cmd = std::process::Command::new(blender);
    cmd.arg("-b");
    if src.extension().map(|e| e.eq_ignore_ascii_case("blend")).unwrap_or(false) {
        cmd.arg(src);
    }
    cmd.arg("--python").arg(&script).arg("--").arg(src).arg(&dst);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // pas de console
    }
    let out = cmd.output().map_err(|e| format!("Blender ne démarre pas : {e}"))?;
    if !dst.is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        let log = String::from_utf8_lossy(&out.stdout);
        let last: Vec<&str> = err.lines().chain(log.lines()).filter(|l| l.contains("Error") || l.contains("rror:")).take(3).collect();
        return Err(format!("Blender n'a pas pu convertir le fichier{}", if last.is_empty() { String::new() } else { format!(" : {}", last.join(" / ")) }));
    }
    Ok(dst)
}

/// Lit un modèle 3D : .glb / .gltf / .obj directement, le reste (.blend, .fbx, .dae, .stl, .ply) par Blender.
pub fn read_model(path: &Path, blender: Option<&Path>) -> Result<Mesh3, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "glb" | "gltf" => read_gltf(path),
        "obj" => read_obj(path),
        _ => {
            let b = blender.ok_or("Blender introuvable : installe Blender (blender.org) ou exporte le modèle en .glb / .obj depuis ton logiciel")?;
            let glb = blender_to_glb(b, path)?;
            let r = read_gltf(&glb);
            let _ = std::fs::remove_file(&glb);
            r
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Prop à partir d'une image

/// Formes d'un prop fait d'une image : (nom, aide).
pub const IMAGE_KINDS: [(&str, &str); 5] = [
    ("Panneau", "Plaque épaisse posée au sol, l'image des deux côtés (enseigne, pancarte, tableau)."),
    ("Affiche murale", "Image fine à accrocher au mur, vue de face ; l'origine est au dos, au milieu."),
    ("Découpe", "Image découpée selon sa transparence, visible des deux côtés (logo, silhouette, plante en 2D)."),
    ("Tapis / sol", "Image posée à plat sur le sol (tapis, logo au sol, marquage)."),
    ("Cube", "Cube avec l'image sur toutes ses faces (caisse, carton)."),
];

/// Texture d'une image : mise à une taille puissance de 2 (au plus MAX_TEX), avec ses niveaux de détail.
fn image_tex(img: &image::RgbaImage) -> TexData {
    let pow = |v: u32| -> u32 {
        let mut p = 4;
        while p * 2 <= v.min(MAX_TEX) {
            p *= 2;
        }
        p
    };
    let (w, h) = (pow(img.width()), pow(img.height()));
    let r = if (w, h) == img.dimensions() { img.clone() } else { image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle) };
    crate::roads::with_mips(w, h, r.into_raw())
}

fn color_tex(c: [u8; 4]) -> TexData {
    crate::roads::with_mips(4, 4, [c[0], c[1], c[2], c[3].max(1)].repeat(16))
}

/// Couleur moyenne d'une image (tranche d'un panneau).
fn avg_color(img: &image::RgbaImage) -> [u8; 4] {
    let (mut s, mut n) = ([0u64; 3], 0u64);
    for p in img.pixels().step_by(7) {
        if p[3] > 128 {
            for k in 0..3 {
                s[k] += p[k] as u64;
            }
            n += 1;
        }
    }
    if n == 0 {
        return [128, 128, 128, 255];
    }
    [(s[0] / n * 7 / 10) as u8, (s[1] / n * 7 / 10) as u8, (s[2] / n * 7 / 10) as u8, 255]
}

/// Ajoute un quadrilatère (a, b, c, d dans le sens inverse des aiguilles vu de devant) à une partie.
fn quad(p: &mut Part, a: V3, b: V3, c: V3, d: V3, uv: [[f32; 2]; 4]) {
    let n = (b - a).cross(c - a).norm();
    let base = p.pos.len() as u32;
    p.pos.extend([a, b, c, d]);
    p.nrm.extend([n; 4]);
    p.uv.extend(uv);
    p.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Boîte centrée en x / y, de z0 à z0 + h, une partie par face (même image).
fn box_faces(p: &mut Part, w: f32, d: f32, z0: f32, h: f32) {
    let (x0, x1, y0, y1, z1) = (-w / 2.0, w / 2.0, -d / 2.0, d / 2.0, z0 + h);
    let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    quad(p, v3(x0, y0, z0), v3(x1, y0, z0), v3(x1, y0, z1), v3(x0, y0, z1), uv); // devant (-y)
    quad(p, v3(x1, y1, z0), v3(x0, y1, z0), v3(x0, y1, z1), v3(x1, y1, z1), uv); // dos
    quad(p, v3(x1, y0, z0), v3(x1, y1, z0), v3(x1, y1, z1), v3(x1, y0, z1), uv); // droite
    quad(p, v3(x0, y1, z0), v3(x0, y0, z0), v3(x0, y0, z1), v3(x0, y1, z1), uv); // gauche
    quad(p, v3(x0, y0, z1), v3(x1, y0, z1), v3(x1, y1, z1), v3(x0, y1, z1), uv); // dessus
    quad(p, v3(x0, y1, z0), v3(x1, y1, z0), v3(x1, y0, z0), v3(x0, y0, z0), uv); // dessous
}

/// Prop fait d'une image : `width` en m (la hauteur suit les proportions de l'image), `thick` l'épaisseur.
/// Devant tourné vers -y (comme les props du jeu à cap 0).
pub fn image_prop(img: &image::RgbaImage, kind: usize, width: f32, thick: f32) -> Mesh3 {
    let w = width.max(0.05);
    let h = w * img.height() as f32 / img.width().max(1) as f32;
    let t = thick.max(0.005);
    let transparent = img.pixels().any(|p| p[3] < 128);
    let mut m = Mesh3 { parts: Vec::new(), images: vec![img.clone()] };
    let mut face = Part { image: Some(0), color: [255; 4], cutout: transparent || kind == 2, ..Default::default() };
    let front = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let (x0, x1) = (-w / 2.0, w / 2.0);
    match kind {
        // panneau : face avant et arrière, tranches unies
        0 => {
            let (y0, y1) = (-t / 2.0, t / 2.0);
            quad(&mut face, v3(x0, y0, 0.0), v3(x1, y0, 0.0), v3(x1, y0, h), v3(x0, y0, h), front);
            quad(&mut face, v3(x1, y1, 0.0), v3(x0, y1, 0.0), v3(x0, y1, h), v3(x1, y1, h), front);
            let mut edge = Part { color: avg_color(img), ..Default::default() };
            let uv = [[0.0; 2]; 4];
            quad(&mut edge, v3(x1, y0, 0.0), v3(x1, y1, 0.0), v3(x1, y1, h), v3(x1, y0, h), uv);
            quad(&mut edge, v3(x0, y1, 0.0), v3(x0, y0, 0.0), v3(x0, y0, h), v3(x0, y1, h), uv);
            quad(&mut edge, v3(x0, y0, h), v3(x1, y0, h), v3(x1, y1, h), v3(x0, y1, h), uv);
            quad(&mut edge, v3(x0, y1, 0.0), v3(x1, y1, 0.0), v3(x1, y0, 0.0), v3(x0, y0, 0.0), uv);
            m.parts.push(face);
            m.parts.push(edge);
        }
        // affiche : une face, origine au dos (contre le mur), centrée en hauteur
        1 => {
            let y = -t;
            quad(&mut face, v3(x0, y, -h / 2.0), v3(x1, y, -h / 2.0), v3(x1, y, h / 2.0), v3(x0, y, h / 2.0), front);
            m.parts.push(face);
        }
        // découpe : deux faces fines
        2 => {
            quad(&mut face, v3(x0, -0.002, 0.0), v3(x1, -0.002, 0.0), v3(x1, -0.002, h), v3(x0, -0.002, h), front);
            quad(&mut face, v3(x1, 0.002, 0.0), v3(x0, 0.002, 0.0), v3(x0, 0.002, h), v3(x1, 0.002, h), front);
            m.parts.push(face);
        }
        // tapis : à plat, le haut de l'image vers +y
        3 => {
            let (y0, y1) = (-h / 2.0, h / 2.0);
            let z = t.min(0.02);
            quad(&mut face, v3(x0, y0, z), v3(x1, y0, z), v3(x1, y1, z), v3(x0, y1, z), front);
            m.parts.push(face);
        }
        // cube
        _ => {
            box_faces(&mut face, w, w, 0.0, w);
            m.parts.push(face);
        }
    }
    m
}

// ---------------------------------------------------------------------------------------------------------------
// Prop fait de formes simples

/// Une forme d'un prop assemblé : genre (0 boîte, 1 cylindre, 2 plaque), taille, décalage, rotation (°),
/// couleur ou texture importée du projet (par son nom).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shape {
    pub kind: u8,
    pub size: [f32; 3],
    pub off: [f32; 3],
    pub yaw: f32,
    pub color: [u8; 3],
    #[serde(default)]
    pub tex: Option<String>,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { kind: 0, size: [1.0, 1.0, 1.0], off: [0.0; 3], yaw: 0.0, color: [180, 170, 160], tex: None }
    }
}

/// Prop assemblé, gardé dans le projet pour être retouché.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PropDef {
    pub name: String,
    pub shapes: Vec<Shape>,
    #[serde(default)]
    pub coll: u8,
}

pub const SHAPE_KINDS: [&str; 3] = ["Boîte", "Cylindre", "Plaque"];

/// Maillage d'un prop assemblé (`textures` : nom, image, taille du motif en m).
pub fn shapes_mesh(shapes: &[Shape], textures: &[(String, image::RgbaImage, f32)]) -> Mesh3 {
    let mut m = Mesh3::default();
    for s in shapes {
        let mut p = Part { color: [s.color[0], s.color[1], s.color[2], 255], ..Default::default() };
        let tile = match s.tex.as_ref().and_then(|n| textures.iter().position(|t| t.0.eq_ignore_ascii_case(n))) {
            Some(k) => {
                let img_i = match m.images.iter().position(|i| i.as_raw() == textures[k].1.as_raw()) {
                    Some(i) => i,
                    None => {
                        m.images.push(textures[k].1.clone());
                        m.images.len() - 1
                    }
                };
                p.image = Some(img_i);
                p.color = [255; 4];
                textures[k].2.max(0.05)
            }
            None => 1.0,
        };
        let [sx, sy, sz] = s.size.map(|v| v.max(0.005));
        match s.kind {
            1 => {
                let seg = 20;
                let (rx, ry) = (sx / 2.0, sy / 2.0);
                let circ = std::f32::consts::PI * (rx + ry);
                for i in 0..seg {
                    let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
                    let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
                    let (p0, p1) = (v3(a0.cos() * rx, a0.sin() * ry, 0.0), v3(a1.cos() * rx, a1.sin() * ry, 0.0));
                    let (u0, u1) = (i as f32 / seg as f32 * circ / tile, (i + 1) as f32 / seg as f32 * circ / tile);
                    quad(&mut p, p0, p1, p1 + v3(0.0, 0.0, sz), p0 + v3(0.0, 0.0, sz), [[u0, sz / tile], [u1, sz / tile], [u1, 0.0], [u0, 0.0]]);
                    // dessus et dessous
                    let uvd = |q: V3| [q.x / tile, q.y / tile];
                    let (c0, c1) = (v3(0.0, 0.0, sz), V3::ZERO);
                    let base = p.pos.len() as u32;
                    p.pos.extend([c0, p0 + c0, p1 + c0]);
                    p.nrm.extend([v3(0.0, 0.0, 1.0); 3]);
                    p.uv.extend([uvd(c0), uvd(p0), uvd(p1)]);
                    p.idx.extend([base, base + 1, base + 2]);
                    let base = p.pos.len() as u32;
                    p.pos.extend([c1, p1, p0]);
                    p.nrm.extend([v3(0.0, 0.0, -1.0); 3]);
                    p.uv.extend([uvd(c1), uvd(p1), uvd(p0)]);
                    p.idx.extend([base, base + 1, base + 2]);
                }
            }
            _ => {
                let sz = if s.kind == 2 { sz.min(0.05) } else { sz };
                box_faces(&mut p, sx, sy, 0.0, sz);
                // motif : taille réelle (chaque face reprend ses dimensions)
                for (fi, q) in p.uv.chunks_exact_mut(4).enumerate() {
                    let (fw, fh) = match fi {
                        0 | 1 => (sx, sz),
                        2 | 3 => (sy, sz),
                        _ => (sx, sy),
                    };
                    for c in q.iter_mut() {
                        c[0] *= fw / tile;
                        c[1] *= fh / tile;
                    }
                }
            }
        }
        // rotation autour de z puis décalage (le bas de la forme sur off.z)
        let (sn, cs) = s.yaw.to_radians().sin_cos();
        let o = v3(s.off[0], s.off[1], s.off[2]);
        for q in p.pos.iter_mut() {
            *q = v3(q.x * cs - q.y * sn, q.x * sn + q.y * cs, q.z) + o;
        }
        for n in p.nrm.iter_mut() {
            *n = v3(n.x * cs - n.y * sn, n.x * sn + n.y * cs, n.z);
        }
        m.parts.push(p);
    }
    m
}

// ---------------------------------------------------------------------------------------------------------------
// Écriture du prop (XML de CodeWalker)

/// Nom de prop valable pour le jeu : minuscules, chiffres, « _ ».
pub fn clean_name(s: &str) -> String {
    let mut n: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '_' => c,
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            _ => '_',
        })
        .collect();
    while n.contains("__") {
        n = n.replace("__", "_");
    }
    let n = n.trim_matches('_').to_string();
    let n = if n.is_empty() { "prop".to_string() } else { n };
    let n = if n.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) { format!("p_{n}") } else { n };
    n.chars().take(48).collect()
}

fn tex_key(name: &str, i: usize) -> u64 {
    (crate::design::custom_key(&format!("{name}#{i}")) & 0xFFFF_FFFF) | 0xD7E5_0000_0000_0000
}

/// Modèle pour la vue et l'export : une partie par morceau de 60 000 indices au plus, textures par clé.
pub fn mesh_model(name: &str, m: &Mesh3) -> (RModel, Vec<u64>, Vec<(u64, TexData)>) {
    let mut texs: Vec<(u64, TexData)> = Vec::new();
    let mut img_keys = Vec::new();
    for (i, img) in m.images.iter().enumerate() {
        let k = tex_key(name, i);
        texs.push((k, image_tex(img)));
        img_keys.push(k);
    }
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let mut geoms = Vec::new();
    let mut keys = Vec::new();
    for p in &m.parts {
        let key = match p.image {
            Some(i) => img_keys[i],
            None => {
                let k = tex_key(name, 1000 + u32::from_le_bytes(p.color) as usize % 1_000_000);
                if !texs.iter().any(|t| t.0 == k) {
                    texs.push((k, color_tex(p.color)));
                }
                k
            }
        };
        let tex = texs.iter().position(|t| t.0 == key).unwrap_or(0) as u32;
        // morceaux : chaque partie du modèle compte ses sommets sur 16 bits
        for chunk in p.idx.chunks(60000) {
            let base = verts.len() as u32;
            let mut remap = std::collections::HashMap::new();
            let first = idx.len() as u32;
            for &i in chunk {
                let j = *remap.entry(i).or_insert_with(|| {
                    let q = p.pos[i as usize];
                    let n = p.nrm.get(i as usize).copied().unwrap_or(v3(0.0, 0.0, 1.0));
                    verts.push(RVert { pos: [q.x, q.y, q.z], nrm: [n.x, n.y, n.z], uv: p.uv.get(i as usize).copied().unwrap_or([0.0, 0.0]), col: [255; 4], uv1: [0.0, 0.0] });
                    verts.len() as u32 - 1 - base
                });
                idx.push(base + j);
            }
            geoms.push(RGeom { first, count: chunk.len() as u32, tex, cutout: p.cutout, blend: 0, glass: false, ..Default::default() });
            keys.push(key);
        }
    }
    (RModel { verts, idx, geoms, embedded_dict: None }, keys, texs)
}

/// Collision du prop : 0 aucune, 1 boîte, 2 forme exacte (boîte au-delà de MAX_COLL_TRIS triangles).
fn collision_bounds(m: &Mesh3, coll: u8) -> Option<String> {
    if coll == 0 {
        return None;
    }
    let (a, b) = m.bounds();
    let (verts, tris): (Vec<V3>, Vec<(u32, u32, u32, u8)>) = if coll == 2 && m.tris() <= MAX_COLL_TRIS {
        let mut v = Vec::new();
        let mut t = Vec::new();
        for p in &m.parts {
            let base = v.len() as u32;
            v.extend(p.pos.iter().copied());
            for c in p.idx.chunks_exact(3) {
                t.push((base + c[0], base + c[1], base + c[2], 0u8));
            }
        }
        (v, t)
    } else {
        // boîte : au moins 2 cm d'épaisseur (affiches, tapis)
        let a = a.min(b - v3(0.02, 0.02, 0.02));
        let v = vec![v3(a.x, a.y, a.z), v3(b.x, a.y, a.z), v3(b.x, b.y, a.z), v3(a.x, b.y, a.z), v3(a.x, a.y, b.z), v3(b.x, a.y, b.z), v3(b.x, b.y, b.z), v3(a.x, b.y, b.z)];
        let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        (v, f.iter().map(|t| (t[0], t[1], t[2], 0u8)).collect())
    };
    if tris.is_empty() {
        return None;
    }
    // type de surface : béton (index habituel de materials.dat, comme ailleurs dans DragoWalker) — **estimé**
    let xml = crate::roads::mesh_collision_xml(&verts, &tris, &[1]);
    let s = xml.find("<Bounds ")?;
    let e = xml.rfind("</Bounds>")? + "</Bounds>".len();
    Some(xml[s..e].to_string())
}

/// Écrit le prop dans `dir` : `<nom>.ydr.xml` (collision intégrée), `<nom>.ytd.xml` + `<nom>/*.dds`,
/// `<nom>.ytyp.xml`. Rend la taille (boîte) du prop.
pub fn write_prop(dir: &Path, name: &str, m: &Mesh3, coll: u8) -> Result<(V3, V3), String> {
    if m.tris() == 0 {
        return Err("modèle vide".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // anciens fichiers de ce prop (textures d'une version précédente)
    let _ = std::fs::remove_dir_all(dir.join(name));
    let (model, keys, texs) = mesh_model(name, m);
    crate::sollumz::write_ytd_xml(dir, name, &texs)?;
    let ydr = crate::sollumz::write_ydr_xml_ex(dir, name, &model, &keys, &texs, Some(name))?;
    if let Some(b) = collision_bounds(m, coll) {
        let s = std::fs::read_to_string(&ydr).map_err(|e| e.to_string())?;
        let s = s.replacen("</Drawable>", &format!("{b}\n</Drawable>"), 1);
        std::fs::write(&ydr, s).map_err(|e| e.to_string())?;
    }
    let (a, b) = m.bounds();
    let c = (a + b) * 0.5;
    let r = (b - a).len() * 0.5;
    let lod = (60.0 + r * 12.0).clamp(60.0, 500.0);
    let ytyp = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapTypes>\n <extensions />\n <archetypes>\n  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{lod}\" />\n   <flags value=\"{PROP_FLAGS}\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"{}\" />\n   <name>{name}</name>\n   <textureDictionary>{name}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{name}</assetName>\n   <extensions />\n  </Item>\n </archetypes>\n <name>{name}</name>\n <dependencies />\n <compositeEntityTypes />\n</CMapTypes>\n",
        a.x,
        a.y,
        a.z,
        b.x,
        b.y,
        b.z,
        c.x,
        c.y,
        c.z,
        (lod * 0.5).max(30.0)
    );
    std::fs::write(dir.join(format!("{name}.ytyp.xml")), ytyp).map_err(|e| e.to_string())?;
    Ok((a, b))
}

/// Exporte un prop seul en ressource FiveM : `<dossier>/<nom>/fxmanifest.lua` + `stream/` (fichiers du jeu
/// convertis par CodeWalker, sinon les XML à convertir).
pub fn export_prop_resource(src_dir: &Path, name: &str, out: &Path, dll: Option<&Path>) -> Result<String, String> {
    let res = out.join(name);
    let stream = res.join("stream");
    std::fs::create_dir_all(&stream).map_err(|e| e.to_string())?;
    for f in [format!("{name}.ydr.xml"), format!("{name}.ytd.xml"), format!("{name}.ytyp.xml")] {
        std::fs::copy(src_dir.join(&f), stream.join(&f)).map_err(|e| format!("{f} : {e}"))?;
    }
    let tsrc = src_dir.join(name);
    let tdst = stream.join(name);
    std::fs::create_dir_all(&tdst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(&tsrc).into_iter().flatten().flatten() {
        let _ = std::fs::copy(e.path(), tdst.join(e.file_name()));
    }
    let man = format!("fx_version 'cerulean'\ngame 'gta5'\nlua54 'yes'\n\ndescription 'Prop {name} (DragoWalker)'\n\nthis_is_a_map 'yes'\n\ndata_file 'DLC_ITYP_REQUEST' 'stream/{name}.ytyp'\n");
    std::fs::write(res.join("fxmanifest.lua"), man).map_err(|e| e.to_string())?;
    match dll {
        Some(d) => match crate::cwbridge::to_binary(d, &stream) {
            Ok((n, errs)) if errs.is_empty() => {
                // XML convertis : retirés (le jeu ne lit que les fichiers binaires)
                for f in [format!("{name}.ydr.xml"), format!("{name}.ytd.xml"), format!("{name}.ytyp.xml")] {
                    if stream.join(f.trim_end_matches(".xml")).is_file() {
                        let _ = std::fs::remove_file(stream.join(&f));
                    }
                }
                if stream.join(format!("{name}.ytd")).is_file() {
                    let _ = std::fs::remove_dir_all(&tdst);
                }
                Ok(format!("Ressource « {name} » prête dans {} ({n} fichier(s) convertis). Ajoute « ensure {name} » au server.cfg ; le prop s'appelle {name}.", res.display()))
            }
            Ok((_, errs)) => Err(format!("conversion : {}", errs.join(" ; "))),
            Err(e) => Err(format!("conversion : {e}")),
        },
        None => Ok(format!("Ressource écrite dans {} en XML (CodeWalker introuvable : convertis le dossier stream avec CodeWalker).", res.display())),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Interface

/// État de l'onglet « Props » du Studio.
#[derive(Default)]
pub struct PropStudio {
    pub img: Option<(PathBuf, image::RgbaImage)>,
    pub img_kind: usize,
    pub img_width: f32,
    pub img_thick: f32,
    pub name: String,
    pub coll: u8,
    pub model: Option<(PathBuf, Mesh3)>,
    pub model_scale: f32,
    pub model_ground: bool,
    /// conversion par Blender en cours (fil à part)
    pub job: Option<std::sync::mpsc::Receiver<Result<Mesh3, String>>>,
    pub job_file: PathBuf,
    /// prop assemblé en cours (rang dans proj.props)
    pub cur: Option<usize>,
    pub last: String,
    pub msg: String,
}

impl DwApp {
    /// Dossier où les props du Studio sont écrits (celui de « Mes modèles »).
    fn props_dir(&self) -> PathBuf {
        PathBuf::from(&self.sollumz.dir)
    }

    /// Écrit un prop dans le dossier des modèles et le recharge (il apparaît dans « Mes modèles »).
    pub fn studio_write_prop(&mut self, name: &str, m: &Mesh3, coll: u8) -> Result<String, String> {
        let name = clean_name(name);
        if let Some(w) = self.game.world.as_ref() {
            if w.archetypes.contains_key(&crate::gta::crypto::joaat(&name)) {
                return Err(format!("« {name} » est déjà un objet du jeu : choisis un autre nom"));
            }
        }
        let (a, b) = write_prop(&self.props_dir(), &name, m, coll)?;
        self.sollumz.refresh(true);
        crate::catalog::set_custom_boxes(self.sollumz.boxes());
        self.game.preview_for = 0;
        self.props.last = name.clone();
        let d = b - a;
        Ok(format!("Prop « {name} » créé ({:.2} × {:.2} × {:.2} m, {} triangles) : il est dans « Mes modèles », prêt à poser ; il part avec l'export de la ressource.", d.x, d.y, d.z, m.tris()))
    }

    /// Pose le dernier prop créé devant la caméra.
    pub fn studio_place_last(&mut self) {
        let n = self.props.last.clone();
        if n.is_empty() {
            return;
        }
        if self.studio.on {
            self.studio_exit(true);
        }
        let p = self.spawn_pos(&n);
        self.library_place(&n, Some(p));
        self.status = format!("« {n} » posé devant la caméra.");
    }

    fn props_msg(&mut self, r: Result<String, String>) {
        match r {
            Ok(s) => {
                self.status = s.clone();
                self.props.msg = s;
            }
            Err(e) => {
                self.status = format!("Prop : {e}");
                self.props.msg = format!("Erreur : {e}");
            }
        }
    }

    /// Onglet « Props » de la fenêtre du Studio.
    pub fn props_studio_ui(&mut self, ui: &mut egui::Ui) {
        use crate::look::{self, ic};
        if self.props.img_width <= 0.0 {
            self.props.img_width = 1.0;
            self.props.img_thick = 0.04;
            self.props.model_scale = 1.0;
            self.props.model_ground = true;
            self.props.coll = 1;
        }
        // conversion Blender finie ?
        if let Some(rx) = &self.props.job {
            match rx.try_recv() {
                Ok(r) => {
                    let f = self.props.job_file.clone();
                    self.props.job = None;
                    match r {
                        Ok(m) => {
                            self.props.msg = format!("Modèle lu : {} partie(s), {} triangles, {} texture(s).", m.parts.len(), m.tris(), m.images.len());
                            self.props.model = Some((f, m));
                        }
                        Err(e) => self.props.msg = format!("Erreur : {e}"),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(_) => self.props.job = None,
            }
        }
        ui.label(RichText::new("Crée tes propres objets FiveM : à partir d'une image, d'un modèle 3D (Blender…) ou de formes simples. Ils sont rangés dans « Mes modèles », se posent sur la carte et partent avec l'export (modèle, textures, collision et type d'objet).").weak());
        ui.horizontal(|ui| {
            ui.label("Nom du prop");
            ui.add(egui::TextEdit::singleline(&mut self.props.name).hint_text("ex. enseigne_garage").desired_width(200.0));
        });
        ui.horizontal(|ui| {
            ui.label("Collision");
            for (k, n, tip) in [(0u8, "Aucune", "On passe au travers (décor léger, affiche)"), (1, "Boîte", "Une boîte autour du prop : simple et sûr"), (2, "Forme exacte", "Les triangles du modèle (jusqu'à 20 000, sinon une boîte)")] {
                if ui.selectable_label(self.props.coll == k, n).on_hover_text(tip).clicked() {
                    self.props.coll = k;
                }
            }
        });
        ui.add_space(4.0);
        // --- image
        look::heading(ui, ic::IMAGE, "Depuis une image");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Choisir une image (PNG, JPEG)…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg"]).pick_file() {
                    match std::fs::read(&p).ok().and_then(|d| image::load_from_memory(&d).ok()) {
                        Some(img) => {
                            if self.props.name.is_empty() {
                                self.props.name = clean_name(&p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                            }
                            self.props.img = Some((p, img.to_rgba8()));
                        }
                        None => self.props.msg = "Erreur : image illisible (PNG ou JPEG)".into(),
                    }
                }
            }
            if let Some((p, img)) = &self.props.img {
                ui.label(RichText::new(format!("{} ({} × {} px)", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), img.width(), img.height())).small());
            }
        });
        if self.props.img.is_some() {
            ui.horizontal_wrapped(|ui| {
                for (k, (n, tip)) in IMAGE_KINDS.iter().enumerate() {
                    if ui.selectable_label(self.props.img_kind == k, *n).on_hover_text(*tip).clicked() {
                        self.props.img_kind = k;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.props.img_width).range(0.05..=50.0).speed(0.02).prefix("largeur ").suffix(" m"));
                if matches!(self.props.img_kind, 0 | 1) {
                    ui.add(egui::DragValue::new(&mut self.props.img_thick).range(0.005..=1.0).speed(0.005).prefix("épaisseur ").suffix(" m"));
                }
                if let Some((_, img)) = &self.props.img {
                    let h = self.props.img_width * img.height() as f32 / img.width().max(1) as f32;
                    ui.label(RichText::new(format!("hauteur {h:.2} m")).small().weak());
                }
            });
            if look::accent(ui, ic::PLUS_CIRCLE, "Créer le prop", "").clicked() {
                let (kind, w, t) = (self.props.img_kind, self.props.img_width, self.props.img_thick);
                let m = self.props.img.as_ref().map(|(_, img)| image_prop(img, kind, w, t));
                if let Some(m) = m {
                    let n = if self.props.name.trim().is_empty() { "mon_image".to_string() } else { self.props.name.clone() };
                    let coll = if kind == 3 && self.props.coll == 2 { 1 } else { self.props.coll };
                    let r = self.studio_write_prop(&n, &m, coll);
                    self.props_msg(r);
                }
            }
        }
        ui.add_space(6.0);
        // --- modèle 3D
        look::heading(ui, ic::CUBE, "Depuis un modèle 3D (Blender…)");
        let blender = self.sollumz.blender.clone();
        ui.label(
            RichText::new(match &blender {
                Some(_) => "Fichiers .blend, .fbx, .dae, .stl, .ply : convertis par Blender (sans fenêtre). .glb, .gltf, .obj : lus directement. Textures de couleur, couleurs et transparence reprises ; 1 unité = 1 m, le haut de Blender reste en haut.",
                None => "Fichiers .glb, .gltf, .obj (exporte-les depuis Blender : Fichier » Exporter » glTF 2.0). Avec Blender installé, les .blend et .fbx sont aussi acceptés.",
            })
            .small()
            .weak(),
        );
        ui.horizontal_wrapped(|ui| {
            let busy = self.props.job.is_some();
            if ui.add_enabled(!busy, egui::Button::new("Choisir un modèle…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Modèle 3D", &["glb", "gltf", "obj", "blend", "fbx", "dae", "stl", "ply"]).pick_file() {
                    if self.props.name.is_empty() {
                        self.props.name = clean_name(&p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                    }
                    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if matches!(ext.as_str(), "glb" | "gltf" | "obj") {
                        match read_model(&p, None) {
                            Ok(m) => {
                                self.props.msg = format!("Modèle lu : {} partie(s), {} triangles, {} texture(s).", m.parts.len(), m.tris(), m.images.len());
                                self.props.model = Some((p, m));
                            }
                            Err(e) => self.props.msg = format!("Erreur : {e}"),
                        }
                    } else {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let b = blender.clone();
                        let pp = p.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(read_model(&pp, b.as_deref()));
                        });
                        self.props.job = Some(rx);
                        self.props.job_file = p;
                        self.props.msg = "Blender convertit le modèle…".into();
                    }
                }
            }
            if busy {
                ui.spinner();
            }
            if let Some((p, m)) = &self.props.model {
                let (a, b) = m.bounds();
                let d = (b - a) * self.props.model_scale;
                ui.label(RichText::new(format!("{} · {:.2} × {:.2} × {:.2} m", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), d.x, d.y, d.z)).small());
            }
        });
        if self.props.model.is_some() {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.props.model_scale).range(0.001..=1000.0).speed(0.01).prefix("échelle ×"));
                ui.checkbox(&mut self.props.model_ground, "Poser au sol (origine en bas au centre)");
            });
            if let Some((_, m)) = &self.props.model {
                if m.tris() > 100_000 {
                    ui.label(RichText::new(format!("{} triangles : lourd pour un prop FiveM (vise moins de 50 000).", m.tris())).small().color(Color32::from_rgb(255, 170, 120)));
                }
            }
            if look::accent(ui, ic::PLUS_CIRCLE, "Créer le prop", "").clicked() {
                if let Some((p, m)) = &self.props.model {
                    let mut m = m.clone();
                    m.fit(self.props.model_scale, self.props.model_ground);
                    let n = if self.props.name.trim().is_empty() { clean_name(&p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()) } else { self.props.name.clone() };
                    let r = self.studio_write_prop(&n, &m, self.props.coll);
                    self.props_msg(r);
                }
            }
        }
        ui.add_space(6.0);
        // --- formes simples
        look::heading(ui, ic::SQUARE, "Assembler des formes");
        ui.label(RichText::new("Boîtes, cylindres et plaques, avec une couleur ou une texture importée (les textures importées du projet, voir « Importer une image » ci-dessous). Gardé dans le projet : tu peux le retoucher et le recréer.").small().weak());
        ui.horizontal_wrapped(|ui| {
            for (i, p) in self.proj.props.iter().enumerate() {
                if ui.selectable_label(self.props.cur == Some(i), &p.name).clicked() {
                    self.props.cur = Some(i);
                    self.props.name = p.name.clone();
                }
            }
            if ui.small_button("+ Nouveau").clicked() {
                self.push_undo();
                let n = if self.props.name.trim().is_empty() { format!("prop_{}", self.proj.props.len() + 1) } else { clean_name(&self.props.name) };
                self.proj.props.push(PropDef { name: n, shapes: vec![Shape::default()], coll: 1 });
                self.props.cur = Some(self.proj.props.len() - 1);
            }
        });
        let tex_names: Vec<String> = self.proj.custom_textures.iter().map(|c| c.name.clone()).collect();
        if let Some(ci) = self.props.cur.filter(|i| *i < self.proj.props.len()) {
            let mut del: Option<usize> = None;
            let mut dup: Option<usize> = None;
            let mut remove_prop = false;
            {
                let def = &mut self.proj.props[ci];
                ui.horizontal(|ui| {
                    ui.label("Nom");
                    ui.add(egui::TextEdit::singleline(&mut def.name).desired_width(180.0));
                    if ui.small_button("Supprimer ce prop").clicked() {
                        remove_prop = true;
                    }
                });
                for (si, s) in def.shapes.iter_mut().enumerate() {
                    egui::Frame::new().fill(Color32::from_rgb(30, 33, 39)).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt(("shape_kind", ci, si)).width(80.0).selected_text(SHAPE_KINDS[(s.kind as usize).min(2)]).show_ui(ui, |ui| {
                                for (k, n) in SHAPE_KINDS.iter().enumerate() {
                                    ui.selectable_value(&mut s.kind, k as u8, *n);
                                }
                            });
                            ui.label("taille");
                            for k in 0..3 {
                                ui.add(egui::DragValue::new(&mut s.size[k]).range(0.005..=50.0).speed(0.01).max_decimals(3));
                            }
                            if ui.small_button("Copier").clicked() {
                                dup = Some(si);
                            }
                            if ui.small_button("✕").on_hover_text("Retirer la forme").clicked() {
                                del = Some(si);
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("position");
                            for k in 0..3 {
                                ui.add(egui::DragValue::new(&mut s.off[k]).range(-50.0..=50.0).speed(0.01).max_decimals(3));
                            }
                            ui.add(egui::DragValue::new(&mut s.yaw).range(-180.0..=180.0).speed(1.0).suffix("°"));
                            ui.color_edit_button_srgb(&mut s.color);
                            let cur = s.tex.clone().unwrap_or_else(|| "couleur".into());
                            egui::ComboBox::from_id_salt(("shape_tex", ci, si)).width(110.0).selected_text(cur).show_ui(ui, |ui| {
                                ui.selectable_value(&mut s.tex, None, "couleur");
                                for n in &tex_names {
                                    ui.selectable_value(&mut s.tex, Some(n.clone()), n);
                                }
                            });
                        });
                    });
                }
                ui.horizontal(|ui| {
                    if ui.small_button("+ Forme").clicked() {
                        let top = def.shapes.iter().map(|s| s.off[2] + s.size[2]).fold(0.0f32, f32::max);
                        def.shapes.push(Shape { off: [0.0, 0.0, top], ..Default::default() });
                    }
                    ui.label(RichText::new("taille : largeur (x), profondeur (y), hauteur (z) ; position du bas de la forme ; le devant du prop est vers -y").small().weak());
                });
            }
            if let Some(si) = dup {
                let s = self.proj.props[ci].shapes[si].clone();
                self.proj.props[ci].shapes.push(Shape { off: [s.off[0] + s.size[0], s.off[1], s.off[2]], ..s });
            }
            if let Some(si) = del {
                self.proj.props[ci].shapes.remove(si);
            }
            if remove_prop {
                self.push_undo();
                self.proj.props.remove(ci);
                self.props.cur = None;
            } else if look::accent(ui, ic::PLUS_CIRCLE, "Créer / mettre à jour le prop", "").clicked() {
                let def = self.proj.props[ci].clone();
                let r = self.build_shape_prop(&def);
                self.props_msg(r);
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Importer une image (PNG) comme texture…").on_hover_text("Ajoute une texture au projet (la même que pour les murs et sols des intérieurs)").clicked() {
                if let Some(paths) = rfd::FileDialog::new().add_filter("Image PNG", &["png"]).pick_files() {
                    for p in paths {
                        if let Ok(mut c) = crate::design::import_png(&p) {
                            let base = c.name.clone();
                            let mut k = 2;
                            while self.proj.custom_textures.iter().any(|x| x.name.eq_ignore_ascii_case(&c.name)) {
                                c.name = format!("{base}_{k}");
                                k += 1;
                            }
                            c.size = 1.0;
                            self.proj.custom_textures.push(c);
                        }
                    }
                    self.shell_sig = 0;
                }
            }
        });
        ui.add_space(6.0);
        // --- dernier prop
        if !self.props.msg.is_empty() {
            ui.label(RichText::new(&self.props.msg).small());
        }
        if !self.props.last.is_empty() {
            let n = self.props.last.clone();
            ui.horizontal_wrapped(|ui| {
                if look::accent(ui, ic::MAP_PIN, &format!("Poser « {n} » sur la carte"), "Devant la caméra ; ensuite il se déplace comme les autres objets").clicked() {
                    self.studio_place_last();
                }
                if ui.button("Exporter seul en ressource FiveM…").on_hover_text("Un dossier de ressource avec ce prop seul (fxmanifest.lua, stream/)").clicked() {
                    if let Some(out) = rfd::FileDialog::new().set_title("Dossier où créer la ressource").pick_folder() {
                        let dll = crate::cwbridge::dll(&self.game.cw_dir);
                        let r = export_prop_resource(&self.props_dir(), &n, &out, dll.as_deref());
                        self.props_msg(r);
                    }
                }
            });
        }
    }

    /// Crée (ou recrée) un prop assemblé de formes.
    pub fn build_shape_prop(&mut self, def: &PropDef) -> Result<String, String> {
        if def.shapes.is_empty() {
            return Err("ajoute au moins une forme".into());
        }
        let texs: Vec<(String, image::RgbaImage, f32)> = self
            .proj
            .custom_textures
            .iter()
            .filter(|c| def.shapes.iter().any(|s| s.tex.as_deref().map(|n| n.eq_ignore_ascii_case(&c.name)).unwrap_or(false)))
            .filter_map(|c| {
                let tex = crate::design::b64_png_tex(&c.png)?;
                let img = image::RgbaImage::from_raw(tex.w, tex.h, tex.levels.first()?.clone())?;
                Some((c.name.clone(), img, c.size))
            })
            .collect();
        let m = shapes_mesh(&def.shapes, &texs);
        let coll = self.props.coll;
        self.studio_write_prop(&def.name, &m, coll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_de_props() {
        assert_eq!(clean_name("Enseigne Garage-2!"), "enseigne_garage_2");
        assert_eq!(clean_name("3D café"), "p_3d_cafe");
        assert_eq!(clean_name("___"), "prop");
    }

    #[test]
    fn prop_image_et_formes() {
        let img = image::RgbaImage::from_pixel(64, 32, image::Rgba([10, 200, 30, 255]));
        let m = image_prop(&img, 0, 2.0, 0.04);
        let (a, b) = m.bounds();
        assert!((b.x - a.x - 2.0).abs() < 1e-4 && (b.z - a.z - 1.0).abs() < 1e-4 && a.z.abs() < 1e-6);
        // devant tourné vers -y
        assert!(m.parts[0].nrm[0].y < -0.99);
        let dir = std::env::temp_dir().join("dw_test_props");
        let _ = std::fs::remove_dir_all(&dir);
        write_prop(&dir, "test_panneau", &m, 2).unwrap();
        let ydr = std::fs::read_to_string(dir.join("test_panneau.ydr.xml")).unwrap();
        assert!(ydr.contains("<Bounds type=\"Composite\">") && ydr.trim_end().ends_with("</Drawable>"));
        assert!(ydr.contains("test_panneau_t"));
        let ytyp = std::fs::read_to_string(dir.join("test_panneau.ytyp.xml")).unwrap();
        assert!(ytyp.contains("<textureDictionary>test_panneau</textureDictionary>"));
        assert!(dir.join("test_panneau.ytd.xml").is_file());
        assert!(std::fs::read_dir(dir.join("test_panneau")).unwrap().count() >= 2);
        // relu comme un modèle de « Mes modèles »
        let mut st = crate::sollumz::SollumzState::new();
        st.dir = dir.to_string_lossy().to_string();
        st.refresh(true);
        let cm = st.find("test_panneau").unwrap_or_else(|| panic!("prop relu : {:?} {:?}", st.errors, st.models.iter().map(|m| m.name.clone()).collect::<Vec<_>>()));
        assert!(cm.missing_textures.is_empty(), "{:?}", cm.missing_textures);
        // formes : boîte + cylindre
        let s = shapes_mesh(&[Shape::default(), Shape { kind: 1, off: [0.0, 0.0, 1.0], ..Default::default() }], &[]);
        let (a, b) = s.bounds();
        assert!((b.z - a.z - 2.0).abs() < 1e-4);
    }

    #[test]
    fn obj_et_gltf() {
        let dir = std::env::temp_dir().join("dw_test_props_obj");
        let _ = std::fs::create_dir_all(&dir);
        // carré dans le plan XZ de Blender (y vers le haut à l'export .obj) : debout dans le jeu
        std::fs::write(dir.join("c.mtl"), "newmtl rouge\nKd 1 0 0\n").unwrap();
        std::fs::write(dir.join("c.obj"), "mtllib c.mtl\nv 0 0 0\nv 1 0 0\nv 1 2 0\nv 0 2 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nusemtl rouge\nf 1/1 2/2 3/3 4/4\n").unwrap();
        let m = read_obj(&dir.join("c.obj")).unwrap();
        assert_eq!(m.tris(), 2);
        assert_eq!(m.parts[0].color, [255, 0, 0, 255]);
        let (a, b) = m.bounds();
        assert!((b.z - a.z - 2.0).abs() < 1e-5, "{a:?} {b:?}");
        // .gltf minimal (triangle, tampon intégré en base64)
        let mut bin = Vec::new();
        for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        let b64 = {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in bin.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    if k <= c.len() {
                        s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
                    } else {
                        s.push('=');
                    }
                }
            }
            s
        };
        let js = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"translation":[0,0,0],"scale":[2,2,2]}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0,0,1,1]}}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,{b64}"}}]}}"#
        );
        std::fs::write(dir.join("t.gltf"), js).unwrap();
        let g = read_gltf(&dir.join("t.gltf")).unwrap();
        assert_eq!(g.tris(), 1);
        assert_eq!(g.parts[0].color, [0, 0, 255, 255]);
        let (a, b) = g.bounds();
        // y du glTF (1 × 2) devient la hauteur
        assert!((b.z - a.z - 2.0).abs() < 1e-5, "{a:?} {b:?}");
    }
}

#[cfg(test)]
mod tests_cw {
    use super::*;

    /// Conversion réelle par CodeWalker (DW_CW_DIR = dossier de CodeWalker) : `cargo test -- --ignored conversion_cw`.
    #[test]
    #[ignore]
    fn conversion_cw() {
        let cw = std::env::var("DW_CW_DIR").unwrap_or_default();
        let dll = crate::cwbridge::dll(&cw).expect("CodeWalker.Core.dll");
        let out = std::path::PathBuf::from(std::env::var("DW_OUT").expect("DW_OUT"));
        let src = out.join("src");
        let img = image::RgbaImage::from_fn(128, 64, |x, y| image::Rgba([(x * 2) as u8, (y * 4) as u8, 120, 255]));
        let m = image_prop(&img, 0, 2.0, 0.05);
        write_prop(&src, "dw_test_enseigne", &m, 1).unwrap();
        let s = shapes_mesh(&[Shape::default(), Shape { kind: 1, size: [0.4, 0.4, 1.0], off: [0.0, 0.0, 1.0], ..Default::default() }], &[]);
        write_prop(&src, "dw_test_formes", &s, 2).unwrap();
        for n in ["dw_test_enseigne", "dw_test_formes"] {
            let r = export_prop_resource(&src, n, &out, Some(&dll));
            println!("{n} : {r:?}");
            let st = out.join(n).join("stream");
            for f in std::fs::read_dir(&st).unwrap().flatten() {
                println!("   {} {} o", f.file_name().to_string_lossy(), f.metadata().map(|m| m.len()).unwrap_or(0));
            }
            assert!(r.is_ok());
            assert!(st.join(format!("{n}.ydr")).is_file() && st.join(format!("{n}.ytd")).is_file() && st.join(format!("{n}.ytyp")).is_file());
            // relu en XML : géométrie, collision et textures bien dans les fichiers du jeu
            let back = out.join(format!("{n}_relu"));
            let _ = std::fs::create_dir_all(&back);
            crate::cwbridge::to_xml(&dll, &[(st.join(format!("{n}.ydr")), back.join(format!("{n}.ydr.xml"))), (st.join(format!("{n}.ytd")), back.join(format!("{n}.ytd.xml")))]).unwrap();
            let x = std::fs::read_dir(&back).unwrap().flatten().find(|f| f.file_name().to_string_lossy().ends_with(".ydr.xml")).map(|f| std::fs::read_to_string(f.path()).unwrap()).unwrap_or_default();
            println!("   relu : {} o, Bounds {}, VertexBuffer {}, Polygons {}", x.len(), x.contains("<Bounds"), x.matches("<VertexBuffer").count(), x.contains("<Polygons"));
            assert!(x.contains("<Bounds") && x.contains("<VertexBuffer"));
        }
    }
}

#[cfg(test)]
mod tests_blender {
    use super::*;

    /// Passage par Blender (installé) : `cargo test -- --ignored par_blender`.
    #[test]
    #[ignore]
    fn par_blender() {
        let b = crate::sollumz::find_blender().expect("Blender");
        let dir = std::env::temp_dir().join("dw_test_props_blender");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("c.obj"), "v 0 0 0\nv 1 0 0\nv 1 2 0\nv 0 2 0\nf 1 2 3 4\n").unwrap();
        let t0 = std::time::Instant::now();
        let glb = blender_to_glb(&b, &dir.join("c.obj")).unwrap();
        let m = read_gltf(&glb).unwrap();
        let (a, z) = m.bounds();
        println!("Blender {} : {:.1} s, {} triangles, {a:?} {z:?}", b.display(), t0.elapsed().as_secs_f32(), m.tris());
        assert_eq!(m.tris(), 2);
        assert!((z.z - a.z - 2.0).abs() < 1e-4);
    }
}
