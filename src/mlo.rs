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
        let (author, description, data_files) = read_manifest(dir);
        let mut res = MloRes { name: name.clone(), dir: dir.to_string_lossy().to_string(), author, description, data_files, credit_files: credit_files(dir), ..Default::default() };
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
            let stem = stem_of(if bin.as_os_str().is_empty() { xml } else { bin });
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
        if let Some(y) = res.ymaps.first() {
            if let Some(i) = self.proj.ymaps.iter().position(|m| &m.name == y) {
                self.target_ymap = i;
            }
        }
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
        self.sync_game_project();
        if let Some(p) = goto {
            self.goto(p, 60.0);
        }
        msg
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
        let todo: Vec<String> = self.proj.mlo_res.iter().map(|r| r.dir.clone()).filter(|d| !self.mlo_state.loaded.contains(d)).collect();
        for (k, d) in todo.into_iter().enumerate() {
            let mut files = Vec::new();
            collect(Path::new(&d), &mut files, 0);
            let (models, _) = load_models(&files, 0x100 * (k as u32 + 1) + 0x20_0000);
            self.mlo_state.models.extend(models);
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
        for r in &self.proj.mlo_res {
            let mut copied = 0;
            for a in &r.assets {
                let p = Path::new(a);
                let Some(fname) = p.file_name() else { continue };
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
            // collisions : recopiées, ou déplacées comme le MLO
            let mv = self.mlo_move(r);
            let mut moved = 0;
            if mv.is_identity() || r.ybns.is_empty() {
                for b in &r.ybns {
                    if let Some(fname) = Path::new(b).file_name() {
                        if std::fs::copy(b, stream.join(fname)).is_ok() {
                            copied += 1;
                        }
                    }
                }
            } else {
                let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo_ybn");
                let _ = std::fs::create_dir_all(&tmp);
                let pairs: Vec<(PathBuf, PathBuf)> = r.ybns.iter().map(|b| (PathBuf::from(b), tmp.join(format!("{}.ybn.xml", stem_of(Path::new(b)))))).collect();
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
