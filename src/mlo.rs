//! Import d'une ressource MLO FiveM (dossier avec fxmanifest.lua et stream/ : .ydr, .ydd, .yft, .ytd, .ytyp,
//! .ymap, .ybn, _manifest.ymf).
//!
//! - Les .ytyp et .ymap binaires sont lus en XML par CodeWalker (MetaXml.GetXml, pont cwbridge) puis ajoutés
//!   au projet : le bâtiment et son intérieur apparaissent et se déplacent comme le reste.
//! - Les modèles (.ydr / .ydd / .yft) et leurs textures (intégrées ou dans les .ytd du dossier) sont lus
//!   directement (mêmes lecteurs que pour le jeu) pour être dessinés.
//! - À l'export, les fichiers du créateur sont recopiés tels quels (modèles, textures, intérieur .ytyp,
//!   _manifest.ymf) ; seul le placement (.ymap) est réécrit. Les collisions (.ybn) sont dans le monde : si le
//!   MLO a été déplacé ou tourné, elles sont relues en XML par CodeWalker (YbnXml.GetXml), déplacées de la même
//!   façon que l'objet de référence (le placement de l'intérieur) puis reconverties.
//! - Le nom de l'auteur (fxmanifest.lua) et les fichiers de licence / lisez-moi sont gardés et recopiés dans
//!   l'export (dossier credits/), avec une ligne de crédit dans le fxmanifest.lua.

use crate::app::DwApp;
use crate::gta::drawable::{RModel, TexData};
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Une ressource MLO importée (gardée dans le projet).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MloRes {
    /// nom du dossier de la ressource
    pub name: String,
    pub dir: String,
    /// auteur (fxmanifest.lua : `author '…'` ou commentaire « Auteur : … »)
    pub author: String,
    pub description: String,
    /// fichiers de crédit / licence à la racine du dossier
    pub credit_files: Vec<String>,
    /// fichiers du créateur recopiés tels quels (modèles, textures, manifeste .ymf…)
    pub assets: Vec<String>,
    /// collisions (dans le monde : déplacées avec le MLO)
    pub ybns: Vec<String>,
    /// (nom du .ytyp dans le projet, fichier binaire d'origine recopié à l'export)
    pub ytyp_files: Vec<(String, String)>,
    /// cartes (.ymap) du projet venues de la ressource
    pub ymaps: Vec<String>,
    /// autres lignes data_file du fxmanifest (type, chemin relatif), recopiées
    pub data_files: Vec<(String, String)>,
    /// objet de référence (placement de l'intérieur) et sa position / orientation à l'import
    pub ref_id: u64,
    pub ref_pos: V3,
    pub ref_rot: Quat,
    /// fichiers illisibles (protégés par Cfx « escrow » ou abîmés)
    pub unreadable: Vec<String>,
    /// scripts déclarés par le fxmanifest (client_script, server_script, shared_script ; chemin relatif)
    #[serde(default)]
    pub scripts: Vec<(String, String)>,
    /// cartes du jeu remplacées par la ressource déjà retirées du projet (voir mlo_vanilla_split)
    #[serde(default)]
    pub vanilla_checked: bool,
}

/// Nom de fichier qui remplace un fichier du jeu (même nom que dans les archives du jeu) : ces fichiers modifient
/// la carte du jeu à l'endroit d'origine du MLO (bâtiment creusé, collision du quartier, occlusion). Avec le jeu
/// chargé, on regarde dans ses archives ; sinon, nom des morceaux de carte du jeu (« id2_… », « hi@id2_… » :
/// deux ou trois lettres, un ou deux chiffres, « _ ») — **estimé**.
pub fn is_vanilla_name(w: Option<&crate::gta::world::GameWorld>, file: &str) -> bool {
    let f = file.to_lowercase();
    // le manifeste (_manifest.ymf) porte ce nom dans toutes les ressources : ce n'est pas un fichier du jeu remplacé
    if f.starts_with("_manifest") {
        return false;
    }
    if let Some(w) = w {
        return w.vfs.files.iter().any(|x| x.name.eq_ignore_ascii_case(&f));
    }
    let n = f.strip_prefix("hi@").unwrap_or(&f);
    let letters = n.chars().take_while(|c| c.is_ascii_lowercase()).count();
    let rest = &n[letters..];
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    (2..=3).contains(&letters) && (1..=2).contains(&digits) && rest[digits..].starts_with('_')
}

/// Remplace les noms inconnus de CodeWalker (« hash_1CEA71A5 ») par les noms des fichiers du dossier quand le
/// hachage correspond (JenkHash du nom en minuscules).
fn unhash(src: &str, names: &[String]) -> String {
    let mut s = src.to_string();
    for n in names {
        let h = crate::gta::crypto::joaat(n);
        for tok in [format!("hash_{h:08X}"), format!("hash_{h:08x}")] {
            if s.contains(&tok) {
                s = s.replace(&tok, n);
            }
        }
    }
    s
}

/// Modèles des MLO importés, prêts pour la carte graphique.
#[derive(Default)]
pub struct MloState {
    /// (hachage du nom, modèle, clé de texture de chaque partie, textures)
    pub models: Vec<(u32, RModel, Vec<u64>, Vec<(u64, TexData)>)>,
    /// dossiers déjà lus
    pub loaded: Vec<String>,
    /// carte graphique à qui les modèles ont été envoyés (adresse), et combien
    pub sent_to: usize,
    pub sent_n: usize,
}

const MODEL_EXT: [&str; 3] = ["ydr", "ydd", "yft"];
/// Fichiers recopiés tels quels (ils ne dépendent pas de la position du MLO).
const COPY_EXT: [&str; 8] = ["ydr", "ydd", "yft", "ytd", "ymf", "ycd", "ypt", "ymt"];

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn stem_of(p: &Path) -> String {
    let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    n.split('.').next().unwrap_or("").to_string()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 6 {
                collect(&p, out, depth + 1);
            }
        } else {
            out.push(p);
        }
    }
}

/// Lit un fichier ressource du jeu posé sur le disque (en-tête RSC7, données compressées).
pub fn read_rsc(path: &Path) -> Option<crate::gta::rpf::Resource> {
    let d = std::fs::read(path).ok()?;
    if d.len() < 16 || &d[0..4] != b"RSC7" {
        return None;
    }
    let sys = u32::from_le_bytes(d[8..12].try_into().ok()?);
    let gfx = u32::from_le_bytes(d[12..16].try_into().ok()?);
    let data = crate::gta::crypto::inflate(&d[16..])?;
    Some(crate::gta::rpf::Resource::new(data, sys, gfx))
}

/// Texte entre guillemets simples ou doubles après `key` dans une ligne de fxmanifest.lua.
fn quoted(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<(char, String)> = None;
    for ch in line.chars() {
        match &mut cur {
            Some((q, s)) => {
                if ch == *q {
                    out.push(std::mem::take(s));
                    cur = None;
                } else {
                    s.push(ch);
                }
            }
            None => {
                if ch == '\'' || ch == '"' {
                    cur = Some((ch, String::new()));
                }
            }
        }
    }
    out
}

/// Auteur, description et lignes data_file du fxmanifest.lua (ou __resource.lua).
fn read_manifest(dir: &Path) -> (String, String, Vec<(String, String)>) {
    let (mut author, mut desc, mut data) = (String::new(), String::new(), Vec::new());
    for n in ["fxmanifest.lua", "__resource.lua"] {
        let Ok(src) = std::fs::read_to_string(dir.join(n)) else { continue };
        for l in src.lines() {
            let t = l.trim();
            let low = t.to_lowercase();
            if low.starts_with("author") {
                if let Some(a) = quoted(t).into_iter().next() {
                    author = a;
                }
            } else if low.starts_with("description") {
                if let Some(a) = quoted(t).into_iter().next() {
                    desc = a;
                }
            } else if low.starts_with("--") && author.is_empty() {
                // commentaire « Auteur : x », « Author: x », « by x »
                let c = t.trim_start_matches('-').trim();
                for key in ["auteur", "author", "créé par", "made by", "by"] {
                    if let Some(i) = c.to_lowercase().find(key) {
                        let rest = c[i + key.len()..].trim_start_matches([' ', ':', '-']).trim();
                        if !rest.is_empty() && i < 3 {
                            author = rest.to_string();
                            break;
                        }
                    }
                }
            } else if low.starts_with("data_file") {
                let q = quoted(t);
                if q.len() >= 2 && !q[0].eq_ignore_ascii_case("DLC_ITYP_REQUEST") && !q[1].contains('*') {
                    data.push((q[0].clone(), q[1].clone()));
                }
            } else if let Some(kind) = ["client_script", "server_script", "shared_script"].iter().find(|k| low.starts_with(*k)) {
                // scripts de la ressource (portes, etc.) : `client_script 'doors.js'` ou `client_scripts { 'a', 'b' }`
                for q in quoted(t).into_iter().filter(|q| !q.contains('*') && !q.starts_with('@')) {
                    data.push((format!("#{kind}"), q));
                }
            }
        }
        break;
    }
    (author, desc, data)
}

/// Fichiers de crédit à la racine : lisez-moi, licence, crédits (.txt, .md, .pdf, ou nom évocateur).
fn credit_files(dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return v };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let n = e.file_name().to_string_lossy().to_lowercase();
        let named = ["readme", "lisez", "license", "licence", "credit", "copyright", "terms", "tos"].iter().any(|k| n.contains(k));
        let doc = n.ends_with(".txt") || n.ends_with(".md") || n.ends_with(".pdf") || n.ends_with(".html");
        if named || doc {
            v.push(e.file_name().to_string_lossy().to_string());
        }
    }
    v.sort();
    v
}

/// Lit les modèles d'un dossier : (hachage du nom, modèle, clés des parties, textures).
/// Textures : celles du modèle, sinon celles des .ytd du dossier (même nom de texture).
pub fn load_models(files: &[PathBuf], base_uid: u32) -> (Vec<(u32, RModel, Vec<u64>, Vec<(u64, TexData)>)>, Vec<String>) {
    use crate::gta::drawable;
    let ytds: Vec<crate::gta::rpf::Resource> = files.iter().filter(|f| ext_of(f) == "ytd").filter_map(|f| read_rsc(f)).collect();
    let mut out = Vec::new();
    let mut bad = Vec::new();
    let mut uid = base_uid;
    for f in files.iter().filter(|f| MODEL_EXT.contains(&ext_of(f).as_str())) {
        let ext = ext_of(f);
        let Some(r) = read_rsc(f) else {
            bad.push(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            continue;
        };
        let stem = stem_of(f);
        let mut entries: Vec<(u32, usize)> = Vec::new();
        match ext.as_str() {
            "ydd" => {
                if let (Some(hashes), Some(draws)) = (r.ptr(r.u64(0x20)), r.ptr(r.u64(0x30))) {
                    let n = r.u16(0x28) as usize;
                    for i in 0..n.min(4096) {
                        if let Some(p) = r.ptr(r.u64(draws + i * 8)) {
                            entries.push((r.u32(hashes + i * 4), p));
                        }
                    }
                }
            }
            e => {
                if let Some(p) = drawable::drawable_pos(&r, e, 0) {
                    entries.push((crate::gta::crypto::joaat(&stem), p));
                }
            }
        }
        for (h, p) in entries {
            let Some(model) = drawable::read_render_drawable(&r, p, crate::gta::loader::MAX_TRIS) else { continue };
            uid += 1;
            let mut textures: Vec<(u64, TexData)> = Vec::new();
            let mut keys = Vec::new();
            for g in &model.geoms {
                if g.tex == 0 {
                    keys.push(0);
                    continue;
                }
                let key = ((0xB000_0000u64 | (uid as u64 & 0x0FFF_FFFF)) << 32) | g.tex as u64;
                if textures.iter().any(|t| t.0 == key) {
                    keys.push(key);
                    continue;
                }
                let t = model
                    .embedded_dict
                    .and_then(|d| drawable::find_texture(&r, d, g.tex, crate::gta::loader::MAX_TEX))
                    .or_else(|| ytds.iter().find_map(|y| drawable::find_texture(y, 0, g.tex, crate::gta::loader::MAX_TEX)));
                match t {
                    Some(t) => {
                        textures.push((key, t));
                        keys.push(key);
                    }
                    None => keys.push(0),
                }
            }
            out.push((h, model, keys, textures));
        }
    }
    (out, bad)
}

/// Boîte d'un modèle (repère du modèle).
fn model_box(m: &RModel) -> (V3, V3) {
    let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for v in &m.verts {
        let p = v3(v.pos[0], v.pos[1], v.pos[2]);
        mn = mn.min(p);
        mx = mx.max(p);
    }
    if mn.x > mx.x {
        (V3::ZERO, V3::ZERO)
    } else {
        (mn, mx)
    }
}

// ------------------------------------------------------------------ collisions déplacées

fn attr_f(line: &str, a: &str) -> Option<f32> {
    let k = format!("{a}=\"");
    let i = line.find(&k)? + k.len();
    let j = line[i..].find('"')? + i;
    line[i..j].trim().parse().ok()
}

fn xyz_of(line: &str) -> Option<V3> {
    Some(v3(attr_f(line, "x")?, attr_f(line, "y")?, attr_f(line, "z")?))
}

fn set_xyz(line: &str, p: V3) -> String {
    let ind: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let tag = line.trim_start().trim_start_matches('<').split(|c: char| c.is_whitespace()).next().unwrap_or("");
    format!("{ind}<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", p.x, p.y, p.z)
}

/// Déplacement rigide : p → q·(p − p0) + p1.
#[derive(Clone, Copy)]
pub struct Move {
    pub q: Quat,
    pub p0: V3,
    pub p1: V3,
}

impl Move {
    fn point(&self, p: V3) -> V3 {
        self.q.rotate(p - self.p0) + self.p1
    }
    pub fn is_identity(&self) -> bool {
        self.q.is_identity() && (self.p1 - self.p0).len() < 0.001
    }
}

/// Déplace une collision au format XML de CodeWalker (YbnXml) :
/// - pièces à sommets (Geometry, GeometryBVH) : sommets tournés, centre GeometryCenter déplacé, boîtes
///   recalculées (CodeWalker recalcule l'arbre de recherche et la quantification à la conversion :
///   BoundGeometry::ReadXml 0x0600410a) ;
/// - formes simples (Box, Sphere, Capsule, Cylinder, Disc…) : déplacées sans toucher à leur matrice. À la
///   conversion, CodeWalker range pour chaque pièce sa boîte telle quelle, sans matrice
///   (BoundComposite::UpdateChildrenBounds 0x06004174 : Min = BoxMin, Max = BoxMax + Margin), alors que l'arbre
///   de recherche applique la matrice (BuildBVH 0x06004172 : BoundingBoxMath.Transform(boîte, Transform)) :
///   une matrice tournée rendrait les deux boîtes différentes. Donc : matrice identité → boîte et centres
///   décalés ; matrice déjà présente → seule sa ligne de position est décalée. Leur orientation propre ne
///   tourne pas (exact pour une sphère) : le nombre de ces formes est rendu pour le compte rendu ;
/// - boîte de l'ensemble recalculée.
/// Rend (XML déplacé, formes simples non tournées).
pub fn move_ybn_xml(src: &str, mv: Move) -> (String, usize) {
    let mut unturned = 0;
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    // blocs enfants : `<Item type="…">` au niveau de <Children> ; la racine seule si ce n'est pas un composite
    let mut blocks: Vec<(usize, usize, String)> = Vec::new();
    let root_composite = lines.iter().any(|l| l.contains("<Bounds type=\"Composite\""));
    if root_composite {
        let Some(c0) = lines.iter().position(|l| l.trim() == "<Children>") else { return (src.to_string(), 0) };
        let mut i = c0 + 1;
        while i < lines.len() && lines[i].trim() != "</Children>" {
            let t = lines[i].trim();
            if t.starts_with("<Item type=\"") {
                let ty = t.trim_start_matches("<Item type=\"").split('"').next().unwrap_or("").to_string();
                let ind = lines[i].len() - lines[i].trim_start().len();
                let mut j = i + 1;
                while j < lines.len() && !(lines[j].trim() == "</Item>" && lines[j].len() - lines[j].trim_start().len() == ind) {
                    j += 1;
                }
                blocks.push((i, j, ty));
                i = j + 1;
            } else {
                i += 1;
            }
        }
    } else if let Some(r) = lines.iter().position(|l| l.trim_start().starts_with("<Bounds type=\"")) {
        let ty = lines[r].trim().trim_start_matches("<Bounds type=\"").split('"').next().unwrap_or("").to_string();
        blocks.push((r, lines.len().saturating_sub(1), ty));
    }
    let (mut all_mn, mut all_mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for (a, b, ty) in &blocks {
        let (a, b) = (*a, *b);
        // matrice de la pièce (identité si absente)
        let mut mat: Option<(usize, [[f32; 4]; 4])> = None;
        for k in a..b {
            if lines[k].trim() == "<CompositeTransform>" && k + 4 < b {
                let mut m = [[0f32; 4]; 4];
                for (r, row) in m.iter_mut().enumerate() {
                    let v: Vec<f32> = lines[k + 1 + r].split_whitespace().filter_map(|s| s.trim_end_matches(',').parse().ok()).collect();
                    for c in 0..4 {
                        row[c] = v.get(c).copied().unwrap_or(if r == c { 1.0 } else { 0.0 });
                    }
                }
                mat = Some((k, m));
                break;
            }
        }
        let identity = mat.map(|(_, m)| (0..4).all(|r| (0..4).all(|c| (m[r][c] - if r == c { 1.0 } else { 0.0 }).abs() < 1e-5))).unwrap_or(true);
        let first = |tag: &str| (a..b).find(|&k| lines[k].trim_start().starts_with(&format!("<{tag} ")));
        if ty.starts_with("Geometry") && identity {
            let gc_line = first("GeometryCenter");
            let gc = gc_line.and_then(|k| xyz_of(lines[k])).unwrap_or(V3::ZERO);
            let gc2 = mv.point(gc);
            if let Some(k) = gc_line {
                out[k] = set_xyz(lines[k], gc2);
            }
            let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
            if let Some(v0) = (a..b).find(|&k| lines[k].trim() == "<Vertices>") {
                let mut k = v0 + 1;
                while k < b && lines[k].trim() != "</Vertices>" {
                    let v: Vec<f32> = lines[k].split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() == 3 {
                        let r = mv.q.rotate(v3(v[0], v[1], v[2]));
                        let ind: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
                        out[k] = format!("{ind}{}, {}, {}", r.x, r.y, r.z);
                        let w = gc2 + r;
                        mn = mn.min(w);
                        mx = mx.max(w);
                    }
                    k += 1;
                }
            }
            if mn.x <= mx.x {
                for (tag, p) in [("BoxMin", mn), ("BoxMax", mx), ("BoxCenter", (mn + mx) * 0.5), ("SphereCenter", (mn + mx) * 0.5)] {
                    if let Some(k) = first(tag) {
                        out[k] = set_xyz(lines[k], p);
                    }
                }
                if let Some(k) = first("SphereRadius") {
                    let ind: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
                    out[k] = format!("{ind}<SphereRadius value=\"{}\" />", (mx - mn).len() * 0.5);
                }
                all_mn = all_mn.min(mn);
                all_mx = all_mx.max(mx);
            }
        } else if let (Some(bmn), Some(bmx)) = (first("BoxMin").and_then(|k| xyz_of(lines[k])), first("BoxMax").and_then(|k| xyz_of(lines[k]))) {
            // forme simple (ou pièce placée par une matrice) : déplacée sans tourner sa matrice (voir plus haut)
            let row = |m: &[[f32; 4]; 4], r: usize| v3(m[r][0], m[r][1], m[r][2]);
            let to_world = |p: V3| match &mat {
                Some((_, m)) if !identity => row(m, 0) * p.x + row(m, 1) * p.y + row(m, 2) * p.z + row(m, 3),
                _ => p,
            };
            let c = to_world((bmn + bmx) * 0.5);
            let delta = mv.point(c) - c;
            if !mv.q.is_identity() && !ty.starts_with("Sphere") {
                unturned += 1;
            }
            match (&mat, identity) {
                (Some((k, m)), false) => {
                    let k = *k;
                    let ind: String = lines[k + 4].chars().take_while(|c| c.is_whitespace()).collect();
                    let p = row(m, 3) + delta;
                    out[k + 4] = format!("{ind}{} {} {} {}", p.x, p.y, p.z, m[3][3]);
                }
                _ => {
                    for tag in ["BoxMin", "BoxMax", "BoxCenter", "SphereCenter"] {
                        if let Some(k) = first(tag) {
                            if let Some(p) = xyz_of(lines[k]) {
                                out[k] = set_xyz(lines[k], p + delta);
                            }
                        }
                    }
                }
            }
            for i in 0..8 {
                let q = v3(if i & 1 == 0 { bmn.x } else { bmx.x }, if i & 2 == 0 { bmn.y } else { bmx.y }, if i & 4 == 0 { bmn.z } else { bmx.z });
                let w = to_world(q) + delta;
                all_mn = all_mn.min(w);
                all_mx = all_mx.max(w);
            }
        }
    }
    // boîte de l'ensemble (avant <Children>)
    if root_composite && all_mn.x <= all_mx.x {
        let end = lines.iter().position(|l| l.trim() == "<Children>").unwrap_or(0);
        let c = (all_mn + all_mx) * 0.5;
        for k in 0..end {
            let t = lines[k].trim_start();
            if t.starts_with("<BoxMin ") {
                out[k] = set_xyz(lines[k], all_mn);
            } else if t.starts_with("<BoxMax ") {
                out[k] = set_xyz(lines[k], all_mx);
            } else if t.starts_with("<BoxCenter ") || t.starts_with("<SphereCenter ") {
                out[k] = set_xyz(lines[k], c);
            } else if t.starts_with("<SphereRadius ") {
                let ind: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
                out[k] = format!("{ind}<SphereRadius value=\"{}\" />", (all_mx - all_mn).len() * 0.5);
            }
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    (s, unturned)
}

/// Marge autour de la boîte de l'intérieur pour garder la façade (m) : côtés, dessous, dessus — **estimé**
/// (devanture, trottoir devant la porte, enseignes et rebord du toit).
pub const FACADE_MARGIN: (f32, f32, f32) = (3.0, 2.0, 6.0);

fn first_f32s(line: &str, n: usize) -> Vec<f32> {
    line.split_whitespace().take(n).filter_map(|s| s.parse().ok()).collect()
}

/// Découpe un modèle au format XML de CodeWalker (YdrXml) : dans chaque partie (Geometries / Item), garde les
/// triangles dont le centre vérifie `keep` (position = 3 premiers nombres de chaque sommet, Layout commençant
/// par Position), renumérote les sommets, recalcule la boîte de la partie (CodeWalker la lit telle quelle :
/// DrawableGeometry::ReadXml 0x06004a42) et celle du modèle ; retire les parties vides et renomme le modèle.
/// Rend (XML, boîte des sommets gardés, triangles gardés, triangles au départ), ou rien si tout est retiré.
pub fn crop_ydr_xml(src: &str, keep: &dyn Fn(V3) -> bool, new_name: &str) -> Option<(String, V3, V3, usize, usize)> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<Option<String>> = lines.iter().map(|l| Some(l.to_string())).collect();
    let ind = |l: &str| l.len() - l.trim_start().len();
    let big = 3.0e38f32;
    let (mut all_mn, mut all_mx) = (v3(big, big, big), v3(-big, -big, -big));
    let (mut kept_n, mut total_n) = (0usize, 0usize);
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "<VertexBuffer>" {
            i += 1;
            continue;
        }
        let vb = i;
        // début de la partie : l'<Item> juste avant
        let Some(gs) = (0..vb).rev().find(|&k| lines[k].trim() == "<Item>") else { i += 1; continue };
        let gind = ind(lines[gs]);
        let find = |from: usize, what: &str| (from..lines.len()).find(|&k| lines[k].trim() == what);
        let (Some(vd0), Some(ib)) = (find(vb, "<Data>").or_else(|| find(vb, "<Data2>")), find(vb, "<IndexBuffer>")) else { return None };
        let Some(vd1) = (vd0 + 1..lines.len()).find(|&k| lines[k].trim().starts_with("</Data")) else { return None };
        let Some(id0) = find(ib, "<Data>") else { return None };
        let Some(id1) = find(id0, "</Data>") else { return None };
        let Some(ge) = (id1..lines.len()).find(|&k| lines[k].trim() == "</Item>" && ind(lines[k]) == gind) else { return None };
        let verts: Vec<&str> = lines[vd0 + 1..vd1].to_vec();
        let pos: Vec<V3> = verts.iter().map(|l| { let f = first_f32s(l, 3); v3(*f.first().unwrap_or(&0.0), *f.get(1).unwrap_or(&0.0), *f.get(2).unwrap_or(&0.0)) }).collect();
        let idx: Vec<usize> = lines[id0 + 1..id1].iter().flat_map(|l| l.split_whitespace().filter_map(|s| s.parse::<usize>().ok())).collect();
        let mut remap: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut order: Vec<usize> = Vec::new();
        let mut new_idx: Vec<usize> = Vec::new();
        for t in idx.chunks_exact(3) {
            total_n += 1;
            if t.iter().any(|&k| k >= pos.len()) {
                continue;
            }
            let c = (pos[t[0]] + pos[t[1]] + pos[t[2]]) * (1.0 / 3.0);
            if !keep(c) {
                continue;
            }
            kept_n += 1;
            for &k in t {
                let n = *remap.entry(k).or_insert_with(|| {
                    order.push(k);
                    order.len() - 1
                });
                new_idx.push(n);
            }
        }
        if new_idx.is_empty() {
            for o in out.iter_mut().take(ge + 1).skip(gs) {
                *o = None;
            }
            i = ge + 1;
            continue;
        }
        let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
        for &k in &order {
            mn = mn.min(pos[k]);
            mx = mx.max(pos[k]);
        }
        all_mn = all_mn.min(mn);
        all_mx = all_mx.max(mx);
        // boîte de la partie
        for k in gs..vb {
            let t = lines[k].trim_start();
            let pad: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
            if t.starts_with("<BoundingBoxMin ") {
                out[k] = Some(format!("{pad}<BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", mn.x, mn.y, mn.z));
            } else if t.starts_with("<BoundingBoxMax ") {
                out[k] = Some(format!("{pad}<BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", mx.x, mx.y, mx.z));
            }
        }
        // sommets gardés (lignes d'origine), puis indices renumérotés (24 par ligne)
        for o in out.iter_mut().take(vd1).skip(vd0 + 1) {
            *o = None;
        }
        out[vd0 + 1] = Some(order.iter().map(|&k| verts[k].to_string()).collect::<Vec<_>>().join("\n"));
        for o in out.iter_mut().take(id1).skip(id0 + 1) {
            *o = None;
        }
        let pad: String = lines[id0].chars().take_while(|c| c.is_whitespace()).collect();
        out[id0 + 1] = Some(new_idx.chunks(24).map(|c| format!("{pad} {}", c.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" "))).collect::<Vec<_>>().join("\n"));
        i = ge + 1;
    }
    if kept_n == 0 {
        return None;
    }
    // modèle (DrawableModel) resté sans partie : retiré, sinon CodeWalker lui donne une boîte infinie
    // (DrawableModel::ReadXml 0x06004a17 part de ±3,4e38 et l'élargit par les boîtes de ses parties) ;
    // liste de modèles (DrawableModelsHigh, Medium…) devenue vide : retirée aussi
    for k in 0..lines.len() {
        if lines[k].trim() != "<Geometries>" || out[k].is_none() {
            continue;
        }
        let gi = ind(lines[k]);
        let Some(end) = (k + 1..lines.len()).find(|&j| lines[j].trim() == "</Geometries>" && ind(lines[j]) == gi) else { continue };
        let any = (k + 1..end).any(|j| out[j].is_some() && lines[j].trim() == "<Item>");
        if any {
            continue;
        }
        let Some(ms) = (0..k).rev().find(|&j| lines[j].trim() == "<Item>" && ind(lines[j]) < gi) else { continue };
        let mi = ind(lines[ms]);
        let Some(me) = (end..lines.len()).find(|&j| lines[j].trim() == "</Item>" && ind(lines[j]) == mi) else { continue };
        for o in out.iter_mut().take(me + 1).skip(ms) {
            *o = None;
        }
    }
    for k in 0..lines.len() {
        let t = lines[k].trim();
        if !(t.starts_with("<DrawableModels") && t.ends_with('>') && !t.ends_with("/>")) || out[k].is_none() {
            continue;
        }
        let tag = &t[1..t.len() - 1];
        let Some(end) = (k + 1..lines.len()).find(|&j| lines[j].trim() == format!("</{tag}>")) else { continue };
        if !(k + 1..end).any(|j| out[j].is_some() && lines[j].trim() == "<Item>") {
            for o in out.iter_mut().take(end + 1).skip(k) {
                *o = None;
            }
        }
    }
    // en-tête : nom, sphère et boîte du modèle
    let header_end = lines.iter().position(|l| l.trim() == "<ShaderGroup>").unwrap_or(0);
    let c = (all_mn + all_mx) * 0.5;
    for k in 0..header_end {
        let t = lines[k].trim_start();
        let pad: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
        if t.starts_with("<Name>") {
            out[k] = Some(format!("{pad}<Name>{new_name}</Name>"));
        } else if t.starts_with("<BoundingSphereCenter ") {
            out[k] = Some(format!("{pad}<BoundingSphereCenter x=\"{}\" y=\"{}\" z=\"{}\" />", c.x, c.y, c.z));
        } else if t.starts_with("<BoundingSphereRadius ") {
            out[k] = Some(format!("{pad}<BoundingSphereRadius value=\"{}\" />", (all_mx - all_mn).len() * 0.5));
        } else if t.starts_with("<BoundingBoxMin ") {
            out[k] = Some(format!("{pad}<BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" />", all_mn.x, all_mn.y, all_mn.z));
        } else if t.starts_with("<BoundingBoxMax ") {
            out[k] = Some(format!("{pad}<BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" />", all_mx.x, all_mx.y, all_mx.z));
        }
    }
    let mut s: String = out.into_iter().flatten().collect::<Vec<_>>().join("\n");
    s.push('\n');
    Some((s, all_mn, all_mx, kept_n, total_n))
}

// ------------------------------------------------------------------ application

impl DwApp {
    pub fn import_mlo_dialog(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_title("Dossier de la ressource MLO (avec fxmanifest.lua / stream)").pick_folder() {
            self.status = self.import_mlo(&d);
        }
    }

    /// Importe le dossier d'une ressource MLO. Rend le message affiché.
    pub fn import_mlo(&mut self, dir: &Path) -> String {
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "mlo".into());
        let mut files = Vec::new();
        collect(dir, &mut files, 0);
        let lower = |p: &Path| p.to_string_lossy().to_lowercase();
        let escrow = files.iter().any(|f| lower(f).ends_with(".fxap"));
        let (author, description, all_data) = read_manifest(dir);
        let (scripts, data_files): (Vec<(String, String)>, Vec<(String, String)>) = all_data.into_iter().partition(|d| d.0.starts_with('#'));
        let scripts = scripts.into_iter().map(|(k, p)| (k.trim_start_matches('#').to_string(), p)).collect();
        let mut res = MloRes { name: name.clone(), dir: dir.to_string_lossy().to_string(), author, description, data_files, scripts, credit_files: credit_files(dir), ..Default::default() };
        // noms des fichiers du dossier : CodeWalker ne connaît pas les noms du créateur et écrit « hash_… »
        let mut known: Vec<String> = files.iter().map(|f| stem_of(f)).filter(|s| !s.is_empty()).collect();
        known.sort();
        known.dedup();
        // .ytyp / .ymap : XML déjà là, sinon lus par CodeWalker
        let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo").join(name.replace(' ', "_"));
        let _ = std::fs::create_dir_all(&tmp);
        let mut pairs: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut xml_of: Vec<(PathBuf, PathBuf, String)> = Vec::new(); // (binaire ou "", xml, extension)
        for f in &files {
            let l = lower(f);
            for ext in ["ytyp", "ymap"] {
                if l.ends_with(&format!(".{ext}")) {
                    let x = tmp.join(format!("{}.{ext}.xml", stem_of(f)));
                    pairs.push((f.clone(), x.clone()));
                    xml_of.push((f.clone(), x, ext.to_string()));
                } else if l.ends_with(&format!(".{ext}.xml")) && !files.iter().any(|g| lower(g).ends_with(&format!("{}.{ext}", stem_of(f))) && lower(g) != l) {
                    xml_of.push((PathBuf::new(), f.clone(), ext.to_string()));
                }
            }
        }
        if !pairs.is_empty() {
            let Some(dll) = crate::cwbridge::dll(&self.game.cw_dir) else {
                return "CodeWalker introuvable : il faut son dossier (onglet Jeu) pour lire les .ytyp / .ymap du MLO.".into();
            };
            if let Err(e) = crate::cwbridge::to_xml(&dll, &pairs) {
                return format!("Lecture du MLO par CodeWalker impossible : {e}");
            }
        }
        self.push_undo();
        let mut n_ent = 0;
        let mut interiors = 0;
        for (bin, xml, ext) in &xml_of {
            let Ok(src) = std::fs::read_to_string(xml) else {
                res.unreadable.push(bin.file_name().or(xml.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                continue;
            };
            let src = unhash(&src, &known);
            let stem = stem_of(if bin.as_os_str().is_empty() { xml } else { bin });
            // carte du jeu remplacée (occlusion, bâtiment creusé…) : gardée telle quelle, liée à l'endroit d'origine
            if ext == "ymap" && !bin.as_os_str().is_empty() && is_vanilla_name(self.game.world.as_deref(), &format!("{stem}.ymap")) {
                res.assets.push(bin.to_string_lossy().to_string());
                continue;
            }
            if ext == "ytyp" {
                match crate::xmlio::read_ytyp(&src, &stem) {
                    Ok(mut t) => {
                        t.name = stem.clone();
                        interiors += t.mlos.len();
                        self.proj.ytyps.retain(|y| !y.name.eq_ignore_ascii_case(&stem));
                        self.proj.ytyps.push(t);
                        res.ytyp_files.push((stem.clone(), if bin.as_os_str().is_empty() { String::new() } else { bin.to_string_lossy().to_string() }));
                    }
                    Err(e) => res.unreadable.push(format!("{stem}.ytyp ({e})")),
                }
            } else {
                match crate::xmlio::read_ymap(&src, &stem) {
                    Ok(mut y) => {
                        y.name = stem.clone();
                        n_ent += y.entities.len();
                        self.proj.ymaps.retain(|m| !m.name.eq_ignore_ascii_case(&stem));
                        self.proj.ymaps.push(y);
                        res.ymaps.push(stem.clone());
                    }
                    Err(e) => res.unreadable.push(format!("{stem}.ymap ({e})")),
                }
            }
        }
        crate::model::bump_ids(self.proj.max_id());
        // objet de référence : le placement de l'intérieur, sinon le premier objet
        let mut refe = None;
        for y in self.proj.ymaps.iter().filter(|y| res.ymaps.contains(&y.name)) {
            if let Some(e) = y.entities.iter().find(|e| e.mlo.is_some()).or(y.entities.first()) {
                if refe.is_none() || e.mlo.is_some() {
                    refe = Some((e.id, e.pos, e.rot));
                    if e.mlo.is_some() {
                        break;
                    }
                }
            }
        }
        if let Some((id, p, q)) = refe {
            res.ref_id = id;
            res.ref_pos = p;
            res.ref_rot = q;
        }
        for f in &files {
            let e = ext_of(f);
            if COPY_EXT.contains(&e.as_str()) {
                res.assets.push(f.to_string_lossy().to_string());
            } else if e == "ybn" {
                res.ybns.push(f.to_string_lossy().to_string());
            }
        }
        // modèles pour la vue
        let (models, bad) = load_models(&files, 0x100 * (self.proj.mlo_res.len() as u32 + 1) + 0x10_0000);
        res.unreadable.extend(bad);
        let n_models = models.len();
        self.mlo_state.models.retain(|m| !models.iter().any(|n| n.0 == m.0));
        self.mlo_state.models.extend(models);
        self.mlo_state.loaded.push(res.dir.clone());
        self.mlo_state.sent_n = 0;
        self.refresh_mlo_boxes();
        // la carte de destination reste une carte du projet (pas une carte du MLO, réécrite à l'export)
        res.vanilla_checked = self.game.world.is_some();
        let goto = refe.map(|r| r.1);
        let author = if res.author.is_empty() { String::new() } else { format!(" de {}", res.author) };
        let mut msg = format!(
            "MLO « {name} »{author} importé : {} carte(s), {n_ent} objet(s), {interiors} intérieur(s), {n_models} modèle(s), {} collision(s).",
            res.ymaps.len(),
            res.ybns.len()
        );
        if !res.credit_files.is_empty() {
            msg += &format!(" Crédits gardés : {}.", res.credit_files.join(", "));
        }
        if escrow || !res.unreadable.is_empty() {
            msg += &format!(" {} fichier(s) illisible(s){}.", res.unreadable.len(), if escrow { " (ressource protégée par Cfx « escrow » : seuls ses propriétaires peuvent l'ouvrir)" } else { "" });
        }
        self.proj.mlo_res.retain(|r| r.dir != res.dir);
        self.proj.mlo_res.push(res);
        self.mlo_vanilla_split();
        self.sync_game_project();
        if let Some(p) = goto {
            self.goto(p, 60.0);
        }
        msg
    }

    /// Cartes du jeu remplacées par un MLO (occlusion, quartier modifié) : elles ne doivent pas être réécrites ni
    /// suivre le MLO ; on les retire du projet (le fichier d'origine sera recopié tel quel) et les objets que la
    /// personne y a posés après l'import (ajoutés à la fin) passent dans une carte du projet. La carte de
    /// destination ne reste jamais sur une carte d'un MLO.
    pub fn mlo_vanilla_split(&mut self) {
        let world_ok = self.game.world.is_some();
        let mut moved_out: Vec<crate::model::Entity> = Vec::new();
        for ri in 0..self.proj.mlo_res.len() {
            if self.proj.mlo_res[ri].vanilla_checked || !world_ok {
                continue;
            }
            let r = self.proj.mlo_res[ri].clone();
            let mut keep = Vec::new();
            for y in &r.ymaps {
                if !is_vanilla_name(self.game.world.as_deref(), &format!("{y}.ymap")) {
                    keep.push(y.clone());
                    continue;
                }
                // objets d'origine : ceux du fichier relu par CodeWalker à l'import
                let cached = std::env::temp_dir().join("dragowalker_cw").join("mlo").join(r.name.replace(' ', "_")).join(format!("{y}.ymap.xml"));
                let orig_n = std::fs::read_to_string(&cached).ok().and_then(|s| crate::xmlio::read_ymap(&s, y).ok()).map(|m| m.entities.len());
                if let Some(i) = self.proj.ymaps.iter().position(|m| &m.name == y) {
                    let m = self.proj.ymaps.remove(i);
                    let n0 = orig_n.unwrap_or(if y.contains("occl") { 0 } else { m.entities.len() });
                    moved_out.extend(m.entities.into_iter().skip(n0));
                }
                let mut files = Vec::new();
                collect(Path::new(&r.dir), &mut files, 0);
                if let Some(f) = files.iter().find(|f| f.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(&format!("{y}.ymap"))).unwrap_or(false)) {
                    self.proj.mlo_res[ri].assets.push(f.to_string_lossy().to_string());
                }
            }
            self.proj.mlo_res[ri].ymaps = keep;
            self.proj.mlo_res[ri].vanilla_checked = true;
        }
        let mlo_maps: std::collections::HashSet<String> = self.proj.mlo_res.iter().flat_map(|r| r.ymaps.iter().cloned()).collect();
        let bad_target = self.proj.ymaps.get(self.target_ymap).map(|y| mlo_maps.contains(&y.name) || y.sample).unwrap_or(true);
        if bad_target || !moved_out.is_empty() {
            let i = match self.proj.ymaps.iter().position(|y| !y.sample && !mlo_maps.contains(&y.name) && !y.from_game) {
                Some(i) => i,
                None => {
                    let n = self.proj.next_ymap_name();
                    self.proj.ymaps.push(crate::model::Ymap::new(&n));
                    self.proj.ymaps.len() - 1
                }
            };
            self.target_ymap = i;
            if !moved_out.is_empty() {
                let n = moved_out.len();
                self.proj.ymaps[i].entities.extend(moved_out);
                self.status = format!("{n} objet(s) posé(s) dans une carte du jeu remplacée par le MLO déplacé(s) dans {}.", self.proj.ymaps[i].name);
                self.sync_game_project();
            }
        }
    }

    /// Tailles des modèles des MLO (sélection, aimant au sol, étendues des cartes).
    pub fn refresh_mlo_boxes(&self) {
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        for t in &self.proj.ytyps {
            for a in &t.raw_archetypes {
                if let (Some(i), Some(j)) = (a.find("<name>"), a.find("</name>")) {
                    if j > i + 6 {
                        let n = a[i + 6..j].trim().to_lowercase();
                        names.insert(crate::gta::crypto::joaat(&n), n);
                    }
                }
            }
        }
        let mut m = std::collections::HashMap::new();
        for (h, model, _, _) in &self.mlo_state.models {
            if let Some(n) = names.get(h) {
                m.insert(n.clone(), model_box(model));
            }
        }
        crate::catalog::set_mlo_boxes(m);
    }

    /// Recharge les modèles des MLO du projet (projet ouvert) et les envoie à la carte graphique.
    pub fn mlo_tick(&mut self) {
        if self.game.world.is_some() && self.proj.mlo_res.iter().any(|r| !r.vanilla_checked) {
            self.mlo_vanilla_split();
        }
        let todo: Vec<String> = self.proj.mlo_res.iter().map(|r| r.dir.clone()).filter(|d| !self.mlo_state.loaded.contains(d)).collect();
        for (k, d) in todo.into_iter().enumerate() {
            let mut files = Vec::new();
            collect(Path::new(&d), &mut files, 0);
            let (models, _) = load_models(&files, 0x100 * (k as u32 + 1) + 0x20_0000);
            self.mlo_state.models.extend(models);
            // projets importés avant : noms « hash_… » remplacés par les noms des fichiers du dossier, pour que le
            // manifeste nomme l'intérieur et sa collision comme CodeWalker (GenerateProjectManifest 0x06001194 :
            // <Interiors> Name = nom de l'intérieur, Bounds = ce même nom)
            let mut known: Vec<String> = files.iter().map(|f| stem_of(f)).filter(|s| !s.is_empty()).collect();
            known.sort();
            known.dedup();
            let fix = |n: &mut String| {
                if let Some(h) = n.to_lowercase().strip_prefix("hash_").and_then(|x| u32::from_str_radix(x, 16).ok()) {
                    if let Some(k) = known.iter().find(|k| crate::gta::crypto::joaat(k) == h) {
                        *n = k.clone();
                    }
                }
            };
            if let Some(r) = self.proj.mlo_res.iter().find(|r| r.dir == d).cloned() {
                for t in self.proj.ytyps.iter_mut().filter(|t| r.ytyp_files.iter().any(|f| f.0.eq_ignore_ascii_case(&t.name))) {
                    for m in t.mlos.iter_mut() {
                        fix(&mut m.name);
                        for e in m.entities.iter_mut() {
                            fix(&mut e.archetype);
                        }
                    }
                    for a in t.raw_archetypes.iter_mut() {
                        *a = unhash(a, &known);
                    }
                }
                for y in self.proj.ymaps.iter_mut().filter(|y| r.ymaps.contains(&y.name)) {
                    for e in y.entities.iter_mut() {
                        fix(&mut e.archetype);
                    }
                }
            }
            self.mlo_state.loaded.push(d);
            self.mlo_state.sent_n = 0;
            self.refresh_mlo_boxes();
        }
        let Some(gpu) = &self.game.gpu else { return };
        let addr = std::sync::Arc::as_ptr(gpu) as usize;
        if self.mlo_state.sent_to == addr && self.mlo_state.sent_n == self.mlo_state.models.len() {
            return;
        }
        if let Ok(mut g) = gpu.lock() {
            for (h, m, k, t) in &self.mlo_state.models {
                g.add_custom(crate::gta::loader::Loaded { arch: *h, model: Some(m.clone()), part_keys: k.clone(), textures: t.clone() });
            }
        }
        self.mlo_state.sent_to = addr;
        self.mlo_state.sent_n = self.mlo_state.models.len();
    }

    /// Noms des .ytyp du projet recopiés depuis un MLO (pas réécrits en XML à l'export).
    pub fn mlo_external_ytyps(&self) -> std::collections::HashSet<String> {
        self.proj.mlo_res.iter().flat_map(|r| r.ytyp_files.iter().filter(|f| !f.1.is_empty() && Path::new(&f.1).is_file()).map(|f| f.0.to_lowercase())).collect()
    }

    /// Déplacement subi par un MLO depuis son import (d'après son objet de référence).
    pub fn mlo_move(&self, r: &MloRes) -> Move {
        let cur = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == r.ref_id);
        match cur {
            Some(e) => Move { q: e.rot.mul(r.ref_rot.inverse()).normalized(), p0: r.ref_pos, p1: e.pos },
            None => Move { q: Quat::IDENTITY, p0: r.ref_pos, p1: r.ref_pos },
        }
    }

    /// Export des MLO importés : fichiers du créateur recopiés, collisions déplacées, crédits.
    /// Rend (compte rendu, lignes à ajouter au fxmanifest.lua).
    pub fn export_mlos(&self, out: &Path, stream: &Path, dll: Option<&Path>) -> (Vec<String>, String) {
        let mut report = Vec::new();
        let mut man = String::new();
        // un seul _manifest.ymf par ressource : celui de DragoWalker (bâtiments copiés) garde son nom
        let ours_ymf = stream.join("_manifest.ymf.xml").exists() || stream.join("_manifest.ymf").exists();
        let w = self.game.world.as_deref();
        for r in &self.proj.mlo_res {
            let mut copied = 0;
            let mv = self.mlo_move(r);
            let moved_mlo = !mv.is_identity();
            // fichiers qui remplacent des fichiers du jeu (quartier creusé, collision et occlusion du quartier) :
            // ils modifient la carte du jeu à l'endroit d'origine ; si le MLO a été déplacé, ils n'ont plus rien à y
            // faire (ils laisseraient un trou là-bas et, déplacés, des murs invisibles ici) : on ne les met pas
            let vanilla = |p: &Path| p.file_name().map(|n| is_vanilla_name(w, &n.to_string_lossy())).unwrap_or(false);
            let mut skipped: Vec<String> = Vec::new();
            for a in &r.assets {
                let p = Path::new(a);
                let Some(fname) = p.file_name() else { continue };
                if moved_mlo && vanilla(p) {
                    skipped.push(fname.to_string_lossy().to_string());
                    // reste d'un export précédent dans ce dossier : retiré, sinon FiveM le chargerait encore
                    let _ = std::fs::remove_file(stream.join(fname));
                    continue;
                }
                let mut dest = stream.join(fname);
                if fname.to_string_lossy().eq_ignore_ascii_case("_manifest.ymf") && ours_ymf {
                    // nom différent pour ne pas écraser celui de DragoWalker — **estimé** (FiveM lit les .ymf du dossier stream)
                    dest = stream.join(format!("{}_manifest.ymf", r.name.replace(' ', "_").to_lowercase()));
                }
                if std::fs::copy(p, &dest).is_ok() {
                    copied += 1;
                }
            }
            for (n, f) in &r.ytyp_files {
                if !f.is_empty() && std::fs::copy(f, stream.join(format!("{n}.ytyp"))).is_ok() {
                    copied += 1;
                }
            }
            // collisions : celle de l'intérieur (même nom que l'intérieur, listée dans <Interiors> du _manifest.ymf)
            // est dans le repère de l'intérieur — relevé sur ma_diner_mlo.ybn : centre (-5,7 ; 0,4 ; 2,4) pour un
            // intérieur posé en (1137 ; -968 ; 45,75) — le jeu la place avec lui : recopiée sans la toucher. Celles du
            // jeu remplacées : voir plus haut. Les autres sont dans le monde : déplacées comme le MLO.
            let mlo_hashes: std::collections::HashSet<u32> = self
                .proj
                .ytyps
                .iter()
                .filter(|t| r.ytyp_files.iter().any(|f| f.0.eq_ignore_ascii_case(&t.name)))
                .flat_map(|t| t.mlos.iter())
                .map(|m| {
                    let n = m.name.to_lowercase();
                    n.strip_prefix("hash_").and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| crate::gta::crypto::joaat(&n))
                })
                .collect();
            let mut moved = 0;
            let mut interior_cols = 0;
            let mut to_move: Vec<&String> = Vec::new();
            for b in &r.ybns {
                let p = Path::new(b);
                let Some(fname) = p.file_name() else { continue };
                let interior = mlo_hashes.contains(&crate::gta::crypto::joaat(&stem_of(p)));
                if moved_mlo && vanilla(p) && !interior {
                    skipped.push(fname.to_string_lossy().to_string());
                    // reste d'un export précédent dans ce dossier : retiré, sinon FiveM le chargerait encore
                    let _ = std::fs::remove_file(stream.join(fname));
                } else if interior || !moved_mlo {
                    if std::fs::copy(p, stream.join(fname)).is_ok() {
                        copied += 1;
                        interior_cols += interior as usize;
                    }
                } else {
                    to_move.push(b);
                }
            }
            if !to_move.is_empty() {
                let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo_ybn");
                let _ = std::fs::create_dir_all(&tmp);
                let pairs: Vec<(PathBuf, PathBuf)> = to_move.iter().map(|b| (PathBuf::from(b), tmp.join(format!("{}.ybn.xml", stem_of(Path::new(b)))))).collect();
                let ok = dll.map(|d| crate::cwbridge::to_xml(d, &pairs).is_ok()).unwrap_or(false);
                for (b, x) in &pairs {
                    match std::fs::read_to_string(x).ok().filter(|_| ok) {
                        Some(src) => {
                            let (moved_xml, unturned) = move_ybn_xml(&src, mv);
                            if std::fs::write(stream.join(format!("{}.ybn.xml", stem_of(b))), moved_xml).is_ok() {
                                moved += 1;
                            }
                            if unturned > 0 {
                                report.push(format!("{unturned} forme(s) simple(s) de collision du MLO « {} » déplacée(s) sans être tournée(s) (boîtes, capsules : CodeWalker range leur boîte sans matrice).", r.name));
                            }
                        }
                        None => {
                            if let Some(fname) = b.file_name() {
                                let _ = std::fs::copy(b, stream.join(fname));
                            }
                            report.push(format!("Collision {} du MLO « {} » pas déplacée (CodeWalker n'a pas pu la lire) : elle reste à l'ancien endroit.", b.display(), r.name));
                        }
                    }
                }
            }
            // façade (modèle du jeu modifié) recopiée sous un nouveau nom à côté de l'intérieur déplacé
            let res_low = r.name.replace(' ', "_").to_lowercase();
            if let Ok(rd) = std::fs::read_dir(stream) {
                for f in rd.flatten() {
                    let n = f.file_name().to_string_lossy().to_lowercase();
                    if (n.starts_with(&format!("{res_low}_x_")) || n.starts_with(&format!("{res_low}_exterieur."))) && f.path().is_file() {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
            if moved_mlo {
                let (rep, lines) = self.export_mlo_exterior(r, mv, stream, dll);
                report.extend(rep);
                man += &lines;
            }
            if !skipped.is_empty() {
                report.push(format!(
                    "MLO « {} » déplacé : {} fichier(s) qui modifient la carte du jeu à son endroit d'origine non mis ({}) — le jeu y reste intact.",
                    r.name,
                    skipped.len(),
                    skipped.join(", ")
                ));
            }
            if interior_cols > 0 {
                report.push(format!("Collision de l'intérieur de « {} » recopiée telle quelle : elle suit l'intérieur toute seule.", r.name));
            }
            // scripts de la ressource (portes…) ; projets importés avant : relus dans le fxmanifest
            let scripts: Vec<(String, String)> = if r.scripts.is_empty() {
                read_manifest(Path::new(&r.dir)).2.into_iter().filter(|d| d.0.starts_with('#')).map(|(k, p)| (k.trim_start_matches('#').to_string(), p)).collect()
            } else {
                r.scripts.clone()
            };
            for (kind, rel) in &scripts {
                let src = Path::new(&r.dir).join(rel);
                let dest = out.join(rel);
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::copy(&src, &dest).is_ok() {
                    man += &format!("{kind} '{}'\n", rel.replace('\\', "/"));
                    if moved_mlo {
                        report.push(format!("Script {rel} du MLO « {} » recopié ; s'il contient des positions (portes verrouillées…), elles sont celles de l'endroit d'origine.", r.name));
                    }
                }
            }
            // autres fichiers déclarés (son d'intérieur, etc.)
            for (ty, rel) in &r.data_files {
                let src = Path::new(&r.dir).join(rel);
                let dest = out.join(rel);
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::copy(&src, &dest).is_ok() {
                    man += &format!("files {{ '{}' }}\ndata_file '{ty}' '{}'\n", rel.replace('\\', "/"), rel.replace('\\', "/"));
                }
            }
            // crédits
            let cdir = out.join("credits").join(r.name.replace(' ', "_"));
            let mut credits = 0;
            if !r.credit_files.is_empty() {
                let _ = std::fs::create_dir_all(&cdir);
                for c in &r.credit_files {
                    if std::fs::copy(Path::new(&r.dir).join(c), cdir.join(c)).is_ok() {
                        credits += 1;
                    }
                }
            }
            let who = if r.author.is_empty() { "auteur non indiqué".to_string() } else { r.author.clone() };
            man = format!("-- MLO « {} » : {who}{}\n", r.name, if credits > 0 { format!(" (licence et crédits dans credits/{})", r.name.replace(' ', "_")) } else { String::new() }) + &man;
            report.push(format!(
                "MLO « {} » ({who}) : {copied} fichier(s) du créateur recopiés{}{}.",
                r.name,
                if moved > 0 { format!(", {moved} collision(s) déplacée(s) avec lui") } else { String::new() },
                if credits > 0 { format!(", {credits} fichier(s) de crédit gardés") } else { String::new() }
            ));
        }
        (report, man)
    }

    /// MLO déplacé : la façade du créateur est souvent un modèle du jeu modifié (ex. id2_16_b3.ydr, le bâtiment du
    /// quartier creusé pour le diner). Le fichier du jeu remplacé n'est plus mis (le jeu reste intact à l'endroit
    /// d'origine) ; à la place, ce modèle est copié sous un nouveau nom « <ressource>_x_<nom> » et posé là où le
    /// MLO a été déplacé, au même décalage qu'à l'origine (placement du jeu de ce modèle le plus proche du MLO,
    /// déplacé comme lui). Type d'objet : boîte, distance d'affichage, drapeaux et dictionnaire de textures du type
    /// du jeu. Collision générée depuis le modèle du créateur, comme pour les bâtiments copiés (buildings.rs).
    /// Gardés : les .ydr / .yft remplacés dont le placement d'origine touche l'intérieur (sphères qui se
    /// chevauchent), hors routes (« _rd ») — **estimé**.
    /// Rend (compte rendu, lignes du fxmanifest).
    fn export_mlo_exterior(&self, r: &MloRes, mv: Move, stream: &Path, dll: Option<&Path>) -> (Vec<String>, String) {
        let mut report = Vec::new();
        let Some(w) = self.game.world.as_deref() else {
            return (vec![format!("Façade du MLO « {} » non copiée : charge le jeu avant d'exporter (il faut le placement d'origine).", r.name)], String::new());
        };
        // intérieur à l'endroit d'origine : centre et rayon, boîte (repère de l'intérieur)
        let inst = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == r.ref_id);
        let mlo_box = inst.and_then(|e| self.proj.find_mlo(&e.archetype)).map(|m| (m.bb_min(), m.bb_max()));
        let (c_mlo, r_mlo) = match mlo_box {
            Some((a, b)) => (r.ref_pos + r.ref_rot.rotate((a + b) * 0.5), (b - a).len() * 0.5),
            None => (r.ref_pos, 20.0),
        };
        // partie du bâtiment gardée : dans la boîte de l'intérieur, élargie de FACADE_MARGIN sur les côtés, en
        // dessous et au-dessus (enseignes, toit)
        let inside = |world: V3| -> bool {
            let Some((a, b)) = mlo_box else { return true };
            let l = r.ref_rot.inverse().rotate(world - r.ref_pos);
            let (side, below, above) = FACADE_MARGIN;
            l.x >= a.x - side && l.x <= b.x + side && l.y >= a.y - side && l.y <= b.y + side && l.z >= a.z - below && l.z <= b.z + above
        };
        let res = r.name.replace(' ', "_").to_lowercase();
        let names = crate::terrain::material_names(Some(w));
        let concrete = names.iter().position(|n| n == "CONCRETE").unwrap_or(1) as u32;
        let (mut archs, mut ents) = (Vec::new(), String::new());
        let big = 3.0e38f32;
        let (mut emn, mut emx) = (v3(big, big, big), v3(-big, -big, -big));
        let mut lod_max = 0.0f32;
        let mut done = Vec::new();
        for a in &r.assets {
            let p = Path::new(a);
            let ext = ext_of(p);
            let stem = stem_of(p);
            if !(ext == "ydr" || ext == "yft") || stem.contains("_rd") || !is_vanilla_name(Some(w), &format!("{stem}.{ext}")) {
                continue;
            }
            let h = crate::gta::crypto::joaat(&stem);
            let Some(arch) = w.archetypes.get(&h) else { continue };
            let ac = (arch.bb_min + arch.bb_max) * 0.5;
            let ar = (arch.bb_max - arch.bb_min).len() * 0.5;
            // placement du jeu de ce modèle le plus proche de l'intérieur
            let Some(e) = w.ents.iter().filter(|e| e.arch == h).min_by(|x, y| x.pos.dist2d(c_mlo).partial_cmp(&y.pos.dist2d(c_mlo)).unwrap_or(std::cmp::Ordering::Equal)) else { continue };
            if (e.pos + e.rot.rotate(ac) - c_mlo).len() > r_mlo + ar + 5.0 {
                continue;
            }
            let Some(model) = self.mlo_state.models.iter().find(|m| m.0 == h).map(|m| &m.1) else {
                report.push(format!("Façade {stem} du MLO « {} » illisible : non copiée.", r.name));
                continue;
            };
            let new = format!("{res}_x_{stem}");
            let s = v3(e.sxy, e.sxy, e.sz);
            // point du modèle (repère du modèle) → monde, à l'endroit d'origine
            let orig = |v: V3| e.pos + e.rot.rotate(v.mulv(s));
            // seulement le bâtiment concerné : modèle relu en XML par CodeWalker, parties hors de la zone retirées
            let mut cropped: Option<(V3, V3)> = None;
            if ext == "ydr" && mlo_box.is_some() {
                if let Some(d) = dll {
                    let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo_ext").join(&res);
                    let _ = std::fs::create_dir_all(&tmp);
                    let x = tmp.join(format!("{stem}.ydr.xml"));
                    let _ = std::fs::remove_file(&x);
                    if crate::cwbridge::to_xml(d, &[(p.to_path_buf(), x.clone())]).is_ok() {
                        if let Some((xml, mn, mx, kept, total)) = std::fs::read_to_string(&x).ok().and_then(|src| crop_ydr_xml(&src, &|v| inside(orig(v)), &new)) {
                            if std::fs::write(stream.join(format!("{new}.ydr.xml")), xml).is_ok() {
                                // textures du modèle : dans le dossier du même nom que le .ydr.xml (cw_convert -ToBinary)
                                let tdir = stream.join(&new);
                                let _ = std::fs::create_dir_all(&tdir);
                                for f in std::fs::read_dir(tmp.join(&stem)).into_iter().flatten().flatten() {
                                    let _ = std::fs::copy(f.path(), tdir.join(f.file_name()));
                                }
                                cropped = Some((mn, mx));
                                report.push(format!("Façade {stem} : {kept} triangle(s) gardé(s) sur {total} (seulement le bâtiment du MLO)."));
                            }
                        }
                    }
                }
            }
            if cropped.is_none() {
                report.push(format!("Façade {stem} copiée en entier (découpe impossible{})", if dll.is_none() { " : CodeWalker introuvable" } else { "" }));
                if std::fs::copy(p, stream.join(format!("{new}.{ext}"))).is_err() {
                    continue;
                }
            }
            let pos = mv.point(e.pos);
            let rot = mv.q.mul(e.rot).normalized();
            let lod = e.lod_dist.max(arch.lod_dist).max(150.0);
            lod_max = lod_max.max(lod);
            let txd = if arch.texture_dict != 0 { w.name_of(arch.texture_dict) } else { String::new() };
            let (bmn, bmx) = cropped.unwrap_or((arch.bb_min, arch.bb_max));
            let (bc, br) = ((bmn + bmx) * 0.5, (bmx - bmn).len() * 0.5);
            archs.push(format!(
                "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{lod}\" />\n   <flags value=\"{}\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{br}\" />\n   <hdTextureDist value=\"{}\" />\n   <name>{new}</name>\n   <textureDictionary>{txd}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>{}</assetType>\n   <assetName>{new}</assetName>\n   <extensions />\n  </Item>",
                arch.flags,
                bmn.x, bmn.y, bmn.z, bmx.x, bmx.y, bmx.z, bc.x, bc.y, bc.z,
                // celle du type du jeu ; absente, la valeur d'un nouveau type de CodeWalker (YtypFile::AddArchetype
                // 0x060030f8 : lodDist 60, hdTextureDist 15, assetType 2 = drawable)
                if arch.hd_texture_dist > 0.0 { arch.hd_texture_dist } else { 15.0 },
                if ext == "yft" { "ASSET_TYPE_FRAGMENT" } else { "ASSET_TYPE_DRAWABLE" }
            ));
            // le fichier range l'inverse de l'orientation réelle (voir model.rs)
            let fq = rot.inverse().normalized();
            ents += &format!(
                "  <Item type=\"CEntityDef\">\n   <archetypeName>{new}</archetypeName>\n   <flags value=\"{}\" />\n   <guid value=\"0\" />\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <rotation x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n   <scaleXY value=\"{}\" />\n   <scaleZ value=\"{}\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"{lod}\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>LODTYPES_DEPTH_ORPHANHD</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
                crate::mech::ENTITY_FLAGS,
                pos.x, pos.y, pos.z, fq.x, fq.y, fq.z, fq.w, e.sxy, e.sz
            );
            for i in 0..8 {
                let k = v3(if i & 1 == 0 { bmn.x } else { bmx.x }, if i & 2 == 0 { bmn.y } else { bmx.y }, if i & 4 == 0 { bmn.z } else { bmx.z });
                let wp = pos + rot.rotate(k.mulv(s));
                emn = emn.min(wp);
                emx = emx.max(wp);
            }
            // collision générée depuis le modèle du créateur, dans le monde (même découpe que le modèle)
            let verts: Vec<V3> = model.verts.iter().map(|v| pos + rot.rotate(v3(v.pos[0], v.pos[1], v.pos[2]).mulv(s))).collect();
            let lv = |i: u32| model.verts.get(i as usize).map(|v| v3(v.pos[0], v.pos[1], v.pos[2])).unwrap_or(V3::ZERO);
            let tris: Vec<(u32, u32, u32, u8)> = model
                .idx
                .chunks_exact(3)
                .filter(|t| cropped.is_none() || inside(orig((lv(t[0]) + lv(t[1]) + lv(t[2])) * (1.0 / 3.0))))
                .map(|t| (t[0], t[1], t[2], 0u8))
                .collect();
            if !tris.is_empty() {
                let _ = std::fs::write(stream.join(format!("{new}_col.ybn.xml")), crate::roads::mesh_collision_xml(&verts, &tris, &[concrete]));
            }
            done.push(stem);
        }
        if done.is_empty() {
            return (report, String::new());
        }
        let ytyp_name = format!("{res}_exterieur");
        let ytyp = crate::model::Ytyp { name: ytyp_name.clone(), mlos: Vec::new(), visible: true, raw_archetypes: archs };
        let _ = std::fs::write(stream.join(format!("{ytyp_name}.ytyp.xml")), crate::xmlio::write_ytyp(&ytyp));
        let vx = |n: &str, p: V3| format!(" <{n} x=\"{}\" y=\"{}\" z=\"{}\" />\n", p.x, p.y, p.z);
        let l = v3(lod_max, lod_max, lod_max);
        let ymap = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{ytyp_name}</name>\n <parent />\n <flags value=\"0\" />\n <contentFlags value=\"1\" />\n{}{}{}{} <entities>\n{ents} </entities>\n <containerLods />\n <boxOccluders />\n <occludeModels />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>{ytyp_name}</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n</CMapData>\n",
            vx("streamingExtentsMin", emn - l),
            vx("streamingExtentsMax", emx + l),
            vx("entitiesExtentsMin", emn),
            vx("entitiesExtentsMax", emx)
        );
        let _ = std::fs::write(stream.join(format!("{ytyp_name}.ymap.xml")), ymap);
        report.push(format!(
            "Façade du MLO « {} » copiée à côté de l'intérieur déplacé ({}) sous un nouveau nom, avec une collision générée ; le bâtiment du jeu reste intact à l'endroit d'origine.",
            r.name,
            done.join(", ")
        ));
        (report, format!("data_file 'DLC_ITYP_REQUEST' 'stream/{ytyp_name}.ytyp'\n"))
    }

    /// Liste des MLO importés (panneau Projet).
    pub fn mlo_list_ui(&mut self, ui: &mut egui::Ui) {
        if self.proj.mlo_res.is_empty() {
            return;
        }
        let mut select: Option<Vec<u64>> = None;
        let mut remove = None;
        let mut goto = None;
        egui::CollapsingHeader::new(RichText::new(format!("MLO importés ({})", self.proj.mlo_res.len())).color(Color32::from_rgb(80, 220, 255))).id_salt("mlo_res").default_open(true).show(ui, |ui| {
            for (i, r) in self.proj.mlo_res.iter().enumerate() {
                ui.label(RichText::new(&r.name).strong());
                let who = if r.author.is_empty() { "auteur non indiqué dans le fxmanifest".to_string() } else { format!("par {}", r.author) };
                ui.label(RichText::new(who).small());
                if !r.credit_files.is_empty() {
                    ui.label(RichText::new(format!("Crédits / licence : {}", r.credit_files.join(", "))).small().weak());
                }
                if !r.unreadable.is_empty() {
                    ui.label(RichText::new(format!("Illisibles : {}", r.unreadable.join(", "))).small().color(Color32::from_rgb(255, 160, 110)));
                }
                let mv = self.mlo_move(r);
                if !mv.is_identity() {
                    ui.label(RichText::new(format!("Déplacé de {:.1} m, tourné de {:.0}° : collisions déplacées à l'export", (mv.p1 - mv.p0).len(), mv.q.heading_deg().min(360.0 - mv.q.heading_deg()))).small().color(Color32::from_rgb(140, 210, 140)));
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Tout sélectionner").on_hover_text("Sélectionne tous ses objets : déplace-les ou tourne-les ensemble avec le gizmo").clicked() {
                        let ids: Vec<u64> = self.proj.ymaps.iter().filter(|y| r.ymaps.contains(&y.name)).flat_map(|y| y.entities.iter().map(|e| e.id)).collect();
                        select = Some(ids);
                    }
                    if ui.small_button("Aller voir").clicked() {
                        goto = Some(mv.p1);
                    }
                    if ui.small_button("Ouvrir le dossier").clicked() {
                        let _ = std::process::Command::new("explorer").arg(&r.dir).spawn();
                    }
                    if ui.small_button("Retirer").on_hover_text("Retire le MLO du projet (ses cartes et son intérieur)").clicked() {
                        remove = Some(i);
                    }
                });
                ui.separator();
            }
        });
        if let Some(ids) = select {
            self.set_selection(ids);
        }
        if let Some(p) = goto {
            self.goto(p, 60.0);
        }
        if let Some(i) = remove {
            self.push_undo();
            let r = self.proj.mlo_res.remove(i);
            self.proj.ymaps.retain(|y| !r.ymaps.contains(&y.name));
            self.proj.ytyps.retain(|t| !r.ytyp_files.iter().any(|f| f.0 == t.name));
            self.target_ymap = self.target_ymap.min(self.proj.ymaps.len().saturating_sub(1));
            self.clean_selection();
            self.sync_game_project();
            self.status = format!("MLO « {} » retiré du projet.", r.name);
        }
    }
}
