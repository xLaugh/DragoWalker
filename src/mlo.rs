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
    /// choix de la fenêtre « Pièces du MLO » : (nom de fichier en minuscules, pris ou non) ; une pièce absente
    /// garde son choix par défaut (piece_default)
    #[serde(default)]
    pub picks: Vec<(String, bool)>,
}

/// Sorte de pièce d'une ressource MLO (fenêtre « Pièces du MLO »).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    /// modèle du créateur (coque, props…)
    Model,
    /// modèle du jeu modifié par le créateur (façade, bâtiment creusé)
    Facade,
    /// morceau de route du jeu modifié (« _rd »)
    Road,
    /// collision de l'intérieur (même nom que l'intérieur : dans son repère)
    InteriorCol,
    /// collision du jeu remplacée (quartier d'origine)
    GameCol,
    /// autre collision du créateur (dans le monde)
    Col,
}

impl PieceKind {
    pub fn label(self) -> &'static str {
        match self {
            PieceKind::Model => "Modèle du créateur",
            PieceKind::Facade => "Façade (bâtiment du jeu modifié)",
            PieceKind::Road => "Route du jeu modifiée",
            PieceKind::InteriorCol => "Collision de l'intérieur",
            PieceKind::GameCol => "Collision du quartier (jeu)",
            PieceKind::Col => "Collision du créateur",
        }
    }
}

/// Une pièce : fichier du créateur, sa sorte.
#[derive(Clone, Debug)]
pub struct Piece {
    pub path: String,
    /// nom de fichier en minuscules
    pub file: String,
    pub stem: String,
    pub kind: PieceKind,
}

/// Choix par défaut d'une pièce. MLO à sa place : tout est pris, comme dans la ressource du créateur. MLO
/// déplacé : les morceaux de route et les collisions du quartier d'origine ne sont pas pris (ils n'ont rien à
/// faire au nouvel endroit, la façade reçoit une collision générée) ; le reste l'est — **estimé**.
pub fn piece_default(kind: PieceKind, moved: bool) -> bool {
    !moved || !matches!(kind, PieceKind::Road | PieceKind::GameCol)
}

impl MloRes {
    /// La pièce est-elle prise à l'export ?
    pub fn taken(&self, p: &Piece, moved: bool) -> bool {
        self.picks.iter().find(|x| x.0 == p.file).map(|x| x.1).unwrap_or_else(|| piece_default(p.kind, moved))
    }

    pub fn set_pick(&mut self, file: &str, v: bool) {
        match self.picks.iter_mut().find(|x| x.0 == file) {
            Some(x) => x.1 = v,
            None => self.picks.push((file.to_string(), v)),
        }
    }
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
    /// fenêtre « Pièces du MLO » ouverte pour ce MLO (nom)
    pub pieces_open: Option<String>,
    /// pièce survolée dans cette fenêtre : (image d'egui où elle l'a été, boîtes à surligner : coins, à l'endroit
    /// d'origine ou non)
    pub hl: (u64, Vec<([V3; 8], bool)>),
    /// fenêtre ouverte : voir le MLO seul, tel qu'il sera exporté (case de la fenêtre)
    pub solo: bool,
    /// l'isolement en cours a été posé par cette fenêtre (à retirer à sa fermeture)
    pub solo_owned: bool,
    /// MLO dont les placements du jeu sont en mémoire : (nom, (fichier, objet du jeu))
    pub place_cache: (String, Vec<(String, Option<usize>)>),
    /// pièce sous la souris dans la vue 3D (nom de fichier)
    pub view_hover: Option<String>,
    /// pièce cliquée dans la vue : montrée dans la liste
    pub focus: Option<String>,
    /// pièces du MLO de la fenêtre : (nom, pièces, jeu chargé quand elles ont été lues)
    pub pieces_cache: (String, Vec<Piece>, bool),
}

/// Rayon contre une boîte donnée par ses 8 coins (coin 0, puis +x = 1, +y = 2, +z = 4) : distance d'entrée.
fn ray_obb(c: &[V3; 8], o: V3, d: V3) -> Option<f32> {
    let c0 = c[0];
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for ax in [c[1] - c0, c[2] - c0, c[4] - c0] {
        let l = ax.len();
        if l < 1e-6 {
            continue;
        }
        let u = ax * (1.0 / l);
        let (oo, dd) = ((o - c0).dot(u), d.dot(u));
        if dd.abs() < 1e-9 {
            if oo < 0.0 || oo > l {
                return None;
            }
            continue;
        }
        let (mut a, mut b) = (-oo / dd, (l - oo) / dd);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
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

/// Part des triangles de la façade gardés par la découpe au-dessus de laquelle la version de loin du jeu (bâtiment
/// entier) lui ressemble assez pour être reprise — **estimé**.
pub const LOD_KEEP_MIN: f32 = 0.6;

/// Drapeaux d'un objet dont la version de loin est (ou non) dans la carte parente : bit 0x8. CodeWalker
/// YmapEntityDef::get_LodInParentYmap 0x06002d03 ((flags >> 3) & 1) ; YmapFile::EnsureEntities 0x06002cb0 ne
/// cherche pas le parent dans la même carte quand il est mis, et EnsureChildYmaps 0x06002cc2 / ConnectToParent
/// 0x06002cc3 le relient alors à l'objet parentIndex de la carte parente. Relevé dans les cartes du jeu
/// (sonde DW_LODFLAGS) : 3 270 objets sur 3 270 avec un parent dans la carte parente ont ce bit, 0 sur 1 031 avec
/// un parent dans la même carte, 0 sur 16 406 sans parent.
pub fn lod_in_parent_flags(flags: u32, in_parent: bool) -> u32 {
    if in_parent {
        flags | crate::mech::LOD_IN_PARENT_YMAP
    } else {
        flags & !crate::mech::LOD_IN_PARENT_YMAP
    }
}

/// Carte (.ymap XML) écrite à la main : nom, parente, flags, contentFlags, (streaming min, max), (objets min, max),
/// objets déjà en XML.
fn raw_ymap(name: &str, parent: &str, flags: u32, content: u32, stream_ext: (V3, V3), ent_ext: (V3, V3), ents: &str) -> String {
    let vx = |n: &str, p: V3| format!(" <{n} x=\"{}\" y=\"{}\" z=\"{}\" />\n", p.x, p.y, p.z);
    let parent = if parent.is_empty() { " <parent />\n".to_string() } else { format!(" <parent>{parent}</parent>\n") };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{name}</name>\n{parent} <flags value=\"{flags}\" />\n <contentFlags value=\"{content}\" />\n{}{}{}{} <entities>\n{ents} </entities>\n <containerLods />\n <boxOccluders />\n <occludeModels />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>{name}</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n</CMapData>\n",
        vx("streamingExtentsMin", stream_ext.0),
        vx("streamingExtentsMax", stream_ext.1),
        vx("entitiesExtentsMin", ent_ext.0),
        vx("entitiesExtentsMax", ent_ext.1)
    )
}

/// Ajoute « cette carte dépend de ces .ytyp » au _manifest.ymf.xml écrit par DragoWalker (export_buildings) ;
/// faux s'il n'y en a pas.
fn add_manifest_imap(stream: &Path, imap: &str, ityps: &[String]) -> bool {
    let path = stream.join("_manifest.ymf.xml");
    let Ok(s) = std::fs::read_to_string(&path) else { return false };
    if ityps.is_empty() || s.contains(&format!("<imapName>{imap}</imapName>")) {
        return true;
    }
    let mut item = format!("    <Item>\n      <imapName>{imap}</imapName>\n      <manifestFlags/>\n      <itypDepArray>\n");
    for t in ityps {
        item += &format!("        <Item>{t}</Item>\n");
    }
    item += "      </itypDepArray>\n    </Item>\n";
    let s = if s.contains("<imapDependencies_2/>") {
        s.replacen("<imapDependencies_2/>", &format!("<imapDependencies_2>\n{item}  </imapDependencies_2>"), 1)
    } else if s.contains("</imapDependencies_2>") {
        s.replacen("  </imapDependencies_2>", &format!("{item}  </imapDependencies_2>"), 1)
    } else {
        return false;
    };
    std::fs::write(&path, s).is_ok()
}

fn first_f32s(line: &str, n: usize) -> Vec<f32> {
    line.split_whitespace().take(n).filter_map(|s| s.parse().ok()).collect()
}

/// Drapeau d'un niveau de détail d'après le nom de sa liste de modèles (DrawableBase::ReadXml 0x06004b94).
fn lod_flags_tag(list: &str) -> Option<&'static str> {
    match list {
        "DrawableModelsHigh" => Some("FlagsHigh"),
        "DrawableModelsMedium" => Some("FlagsMed"),
        "DrawableModelsLow" => Some("FlagsLow"),
        "DrawableModelsVeryLow" => Some("FlagsVlow"),
        _ => None,
    }
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
    // niveaux de détail vidés : leur drapeau passe à 0, comme dans les fichiers du jeu pour un niveau absent
    // (CodeWalker relit FlagsHigh/Med/Low/Vlow tels quels : DrawableBase::ReadXml 0x06004b94, IL_00d0–IL_011f ;
    // listes DrawableModelsHigh/Medium/Low/VeryLow, IL_01af–IL_01f1)
    let mut emptied: Vec<&str> = Vec::new();
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
            if let Some(f) = lod_flags_tag(tag) {
                emptied.push(f);
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
        } else if let Some(f) = emptied.iter().find(|f| t.starts_with(&format!("<{f} "))) {
            out[k] = Some(format!("{pad}<{f} value=\"0\" />"));
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
                g.add_custom(crate::gta::loader::Loaded { arch: *h, model: Some(m.clone()), part_keys: k.clone(), layer_keys: Vec::new(), textures: t.clone() });
            }
        }
        self.mlo_state.sent_to = addr;
        self.mlo_state.sent_n = self.mlo_state.models.len();
    }

    /// Noms des .ytyp du projet recopiés depuis un MLO (pas réécrits en XML à l'export).
    pub fn mlo_external_ytyps(&self) -> std::collections::HashSet<String> {
        self.proj.mlo_res.iter().flat_map(|r| r.ytyp_files.iter().filter(|f| !f.1.is_empty() && Path::new(&f.1).is_file()).map(|f| f.0.to_lowercase())).collect()
    }

    /// Hachages des intérieurs définis par les .ytyp de la ressource (une collision du même nom est celle de
    /// l'intérieur, dans son repère).
    fn mlo_hashes(&self, r: &MloRes) -> std::collections::HashSet<u32> {
        self.proj
            .ytyps
            .iter()
            .filter(|t| r.ytyp_files.iter().any(|f| f.0.eq_ignore_ascii_case(&t.name)))
            .flat_map(|t| t.mlos.iter())
            .map(|m| {
                let n = m.name.to_lowercase();
                n.strip_prefix("hash_").and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| crate::gta::crypto::joaat(&n))
            })
            .collect()
    }

    /// Pièces d'une ressource MLO : ses modèles et ses collisions, rangés par sorte.
    pub fn mlo_pieces(&self, r: &MloRes) -> Vec<Piece> {
        let w = self.game.world.as_deref();
        let hashes = self.mlo_hashes(r);
        let mut out = Vec::new();
        for a in r.assets.iter().chain(r.ybns.iter()) {
            let p = Path::new(a);
            let ext = ext_of(p);
            let stem = stem_of(p);
            let file = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let vanilla = is_vanilla_name(w, &file);
            let kind = if ext == "ybn" {
                if hashes.contains(&crate::gta::crypto::joaat(&stem)) {
                    PieceKind::InteriorCol
                } else if vanilla {
                    PieceKind::GameCol
                } else {
                    PieceKind::Col
                }
            } else if MODEL_EXT.contains(&ext.as_str()) {
                if !vanilla {
                    PieceKind::Model
                } else if stem.contains("_rd") {
                    PieceKind::Road
                } else {
                    PieceKind::Facade
                }
            } else {
                continue;
            };
            out.push(Piece { path: a.clone(), file, stem, kind });
        }
        out.sort_by(|a, b| (a.kind as u8, &a.file).cmp(&(b.kind as u8, &b.file)));
        out
    }

    /// Intérieur à l'endroit d'origine : (centre, rayon, boîte dans le repère de l'intérieur).
    fn mlo_sphere(&self, r: &MloRes) -> (V3, f32, Option<(V3, V3)>) {
        let inst = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == r.ref_id);
        let mlo_box = inst.and_then(|e| self.proj.find_mlo(&e.archetype)).map(|m| (m.bb_min(), m.bb_max()));
        match mlo_box {
            Some((a, b)) => (r.ref_pos + r.ref_rot.rotate((a + b) * 0.5), (b - a).len() * 0.5, mlo_box),
            None => (r.ref_pos, 20.0, None),
        }
    }

    /// Placement du jeu d'un modèle du jeu modifié par le créateur : celui qui est le plus proche de l'intérieur
    /// (à l'endroit d'origine), s'il touche l'intérieur (sphères qui se chevauchent, + 5 m) — **estimé**.
    fn mlo_game_placement(&self, r: &MloRes, stem: &str) -> Option<usize> {
        let w = self.game.world.as_deref()?;
        let (c_mlo, r_mlo, _) = self.mlo_sphere(r);
        let h = crate::gta::crypto::joaat(stem);
        let arch = w.archetypes.get(&h)?;
        let ac = (arch.bb_min + arch.bb_max) * 0.5;
        let ar = (arch.bb_max - arch.bb_min).len() * 0.5;
        let (ei, e) = w.ents.iter().enumerate().filter(|(_, e)| e.arch == h).min_by(|x, y| x.1.pos.dist2d(c_mlo).partial_cmp(&y.1.pos.dist2d(c_mlo)).unwrap_or(std::cmp::Ordering::Equal))?;
        ((e.pos + e.rot().rotate(ac) - c_mlo).len() <= r_mlo + ar + 5.0).then_some(ei)
    }

    /// Boîtes à surligner pour une pièce : (coins, true = à l'endroit d'origine).
    fn piece_boxes(&self, r: &MloRes, p: &Piece) -> Vec<([V3; 8], bool)> {
        let mv = self.mlo_move(r);
        let mut out = Vec::new();
        match p.kind {
            PieceKind::Facade | PieceKind::Road => {
                if let (Some(w), Some(ei)) = (self.game.world.as_deref(), self.mlo_game_placement(r, &p.stem)) {
                    let c = w.corners(ei as u32);
                    out.push((c, true));
                    if !mv.is_identity() {
                        out.push((c.map(|v| mv.point(v)), false));
                    }
                }
            }
            PieceKind::Model => {
                for y in self.proj.ymaps.iter().filter(|y| r.ymaps.contains(&y.name)) {
                    for e in y.entities.iter().filter(|e| e.archetype.eq_ignore_ascii_case(&p.stem)) {
                        out.push((e.corners(&self.proj), false));
                    }
                }
            }
            PieceKind::InteriorCol => {
                if let Some(e) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == r.ref_id) {
                    out.push((e.corners(&self.proj), false));
                }
            }
            PieceKind::GameCol | PieceKind::Col => {}
        }
        out
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
            // test en jeu : MLO laissé de côté — ses fichiers d'un export précédent sont retirés du dossier stream
            if self.export_skip.contains(&format!("mlo:{}", r.name)) {
                let res_low = r.name.replace(' ', "_").to_lowercase();
                let mut names: Vec<String> = r.assets.iter().chain(r.ybns.iter()).filter_map(|a| Path::new(a).file_name().map(|n| n.to_string_lossy().to_lowercase())).collect();
                names.extend(r.ytyp_files.iter().map(|f| format!("{}.ytyp", f.0.to_lowercase())));
                names.extend(r.ymaps.iter().map(|y| format!("{}.ymap", y.to_lowercase())));
                // (le _manifest.ymf de DragoWalker, écrit juste avant, n'est pas à lui)
                names.retain(|n| n != "_manifest.ymf");
                names.push(format!("{res_low}_manifest.ymf"));
                for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
                    let n = f.file_name().to_string_lossy().to_lowercase();
                    let base = n.trim_end_matches(".xml").to_string();
                    if f.path().is_file() && (names.contains(&base) || n.starts_with(&format!("{res_low}_x_")) || n.starts_with(&format!("{res_low}_exterieur"))) {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
                report.push(format!("MLO « {} » laissé de côté pour ce test.", r.name));
                continue;
            }
            let mut copied = 0;
            let mv = self.mlo_move(r);
            let moved_mlo = !mv.is_identity();
            let res_low = r.name.replace(' ', "_").to_lowercase();
            // copies (façade, collisions renommées, version de loin) d'un export précédent : réécrites plus bas
            if let Ok(rd) = std::fs::read_dir(stream) {
                for f in rd.flatten() {
                    let n = f.file_name().to_string_lossy().to_lowercase();
                    if (n.starts_with(&format!("{res_low}_x_")) || n.starts_with(&format!("{res_low}_exterieur"))) && n.ends_with(".xml") && f.path().is_file() {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
            // pièces décochées dans la fenêtre « Pièces du MLO » : pas mises (restes d'un export précédent retirés)
            let pieces = self.mlo_pieces(r);
            let left_out: Vec<&Piece> = pieces.iter().filter(|p| !r.taken(p, moved_mlo)).collect();
            let is_left = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).map(|n| left_out.iter().any(|x| x.file == n)).unwrap_or(false);
            let mut unticked: Vec<String> = Vec::new();
            // fichiers qui remplacent des fichiers du jeu (quartier creusé, collision et occlusion du quartier) :
            // ils modifient la carte du jeu à l'endroit d'origine ; si le MLO a été déplacé, ils n'ont plus rien à y
            // faire (ils laisseraient un trou là-bas et, déplacés, des murs invisibles ici) : on ne les met pas
            let vanilla = |p: &Path| p.file_name().map(|n| is_vanilla_name(w, &n.to_string_lossy())).unwrap_or(false);
            let mut skipped: Vec<String> = Vec::new();
            for a in &r.assets {
                let p = Path::new(a);
                let Some(fname) = p.file_name() else { continue };
                if is_left(p) && !(moved_mlo && vanilla(p)) {
                    unticked.push(fname.to_string_lossy().to_string());
                    let _ = std::fs::remove_file(stream.join(fname));
                    continue;
                }
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
            let mlo_hashes = self.mlo_hashes(r);
            let mut moved = 0;
            let mut interior_cols = 0;
            // (collision, nom du fichier écrit) : une collision du quartier d'origine cochée garde son contenu mais
            // pas son nom du jeu (elle remplacerait celle du jeu là-bas) : « <ressource>_x_<nom> »
            let mut to_move: Vec<(&String, String)> = Vec::new();
            for b in &r.ybns {
                let p = Path::new(b);
                let Some(fname) = p.file_name() else { continue };
                let interior = mlo_hashes.contains(&crate::gta::crypto::joaat(&stem_of(p)));
                if is_left(p) && !(moved_mlo && vanilla(p) && !interior) {
                    unticked.push(fname.to_string_lossy().to_string());
                    let _ = std::fs::remove_file(stream.join(fname));
                    continue;
                }
                if moved_mlo && vanilla(p) && !interior && r.picks.iter().any(|x| x.1 && x.0 == fname.to_string_lossy().to_lowercase()) {
                    // reste d'un export précédent sous le nom du jeu : retiré
                    let _ = std::fs::remove_file(stream.join(fname));
                    to_move.push((b, format!("{res_low}_x_{}", stem_of(p))));
                } else if moved_mlo && vanilla(p) && !interior {
                    skipped.push(fname.to_string_lossy().to_string());
                    // reste d'un export précédent dans ce dossier : retiré, sinon FiveM le chargerait encore
                    let _ = std::fs::remove_file(stream.join(fname));
                } else if interior || !moved_mlo {
                    if std::fs::copy(p, stream.join(fname)).is_ok() {
                        copied += 1;
                        interior_cols += interior as usize;
                    }
                } else {
                    to_move.push((b, stem_of(p)));
                }
            }
            if !to_move.is_empty() {
                let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo_ybn");
                let _ = std::fs::create_dir_all(&tmp);
                let pairs: Vec<(PathBuf, PathBuf)> = to_move.iter().map(|(b, _)| (PathBuf::from(b), tmp.join(format!("{}.ybn.xml", stem_of(Path::new(b)))))).collect();
                let ok = dll.map(|d| crate::cwbridge::to_xml(d, &pairs).is_ok()).unwrap_or(false);
                for ((b, x), (_, out_name)) in pairs.iter().zip(to_move.iter()) {
                    match std::fs::read_to_string(x).ok().filter(|_| ok) {
                        Some(src) => {
                            let (moved_xml, unturned) = move_ybn_xml(&src, mv);
                            if std::fs::write(stream.join(format!("{out_name}.ybn.xml")), moved_xml).is_ok() {
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
            if moved_mlo {
                let chosen: Vec<&Piece> = pieces.iter().filter(|p| matches!(p.kind, PieceKind::Facade | PieceKind::Road) && r.taken(p, true)).collect();
                let (rep, lines) = self.export_mlo_exterior(r, mv, stream, dll, &chosen);
                report.extend(rep);
                man += &lines;
            }
            if !unticked.is_empty() {
                report.push(format!("MLO « {} » : {} pièce(s) décochée(s) non mise(s) ({}).", r.name, unticked.len(), unticked.join(", ")));
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
    /// Gardés : les .ydr / .yft remplacés cochés dans « Pièces du MLO » (par défaut les façades, pas les routes
    /// « _rd ») dont le placement d'origine touche l'intérieur (sphères qui se chevauchent) — **estimé**.
    /// Version de loin : celle du jeu (parent LOD du placement d'origine, lods.rs) déplacée comme la façade, dans
    /// une carte « <ressource>_exterieur_lod » parente, si ce parent ne sert qu'à ce bâtiment et que la découpe a
    /// gardé au moins LOD_KEEP_MIN des triangles ; sinon la façade reste affichée jusqu'à la distance de ce parent
    /// (1 500 m au plus).
    /// Rend (compte rendu, lignes du fxmanifest).
    fn export_mlo_exterior(&self, r: &MloRes, mv: Move, stream: &Path, dll: Option<&Path>, chosen: &[&Piece]) -> (Vec<String>, String) {
        let mut report = Vec::new();
        let Some(w) = self.game.world.as_deref() else {
            return (vec![format!("Façade du MLO « {} » non copiée : charge le jeu avant d'exporter (il faut le placement d'origine).", r.name)], String::new());
        };
        // intérieur à l'endroit d'origine : boîte (repère de l'intérieur)
        let (_, _, mlo_box) = self.mlo_sphere(r);
        // versions de loin copiées (carte « _lod ») : entités XML, coins, distance la plus grande
        let mut lod_ents = String::new();
        let mut n_lod = 0i32;
        let (mut lmn, mut lmx) = (v3(3.0e38, 3.0e38, 3.0e38), v3(-3.0e38, -3.0e38, -3.0e38));
        let mut lod_far = 0.0f32;
        let mut lod_ytyps: Vec<String> = Vec::new();
        let mut lod_notes: Vec<String> = Vec::new();
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
        for piece in chosen {
            let p = Path::new(&piece.path);
            let ext = ext_of(p);
            let stem = piece.stem.clone();
            if !(ext == "ydr" || ext == "yft") {
                continue;
            }
            let h = crate::gta::crypto::joaat(&stem);
            let Some(arch) = w.archetypes.get(&h) else { continue };
            // placement du jeu de ce modèle le plus proche de l'intérieur, s'il le touche
            let Some(ei) = self.mlo_game_placement(r, &stem) else {
                report.push(format!("Pièce {stem} du MLO « {} » cochée mais trop loin de l'intérieur : non copiée.", r.name));
                continue;
            };
            let e = &w.ents[ei];
            let Some(model) = self.mlo_state.models.iter().find(|m| m.0 == h).map(|m| &m.1) else {
                report.push(format!("Façade {stem} du MLO « {} » illisible : non copiée.", r.name));
                continue;
            };
            let new = format!("{res}_x_{stem}");
            let s = v3(e.sxy(), e.sxy(), e.sz());
            // point du modèle (repère du modèle) → monde, à l'endroit d'origine
            let orig = |v: V3| e.pos + e.rot().rotate(v.mulv(s));
            // seulement le bâtiment concerné : modèle relu en XML par CodeWalker, parties hors de la zone retirées
            let mut cropped: Option<(V3, V3)> = None;
            // part des triangles gardés par la découpe (1 = bâtiment entier)
            let mut kept_ratio = 1.0f32;
            if ext == "ydr" && mlo_box.is_some() {
                if let Some(d) = dll {
                    let tmp = std::env::temp_dir().join("dragowalker_cw").join("mlo_ext").join(&res);
                    let _ = std::fs::create_dir_all(&tmp);
                    // relu une seule fois (cache de cwbridge::to_xml : XML plus récent que le .ydr)
                    let x = tmp.join(format!("{stem}.ydr.xml"));
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
                                kept_ratio = kept as f32 / total.max(1) as f32;
                                report.push(format!("Façade {stem} : {kept} triangle(s) gardé(s) sur {total} (seulement le bâtiment du MLO)."));
                            }
                        }
                    }
                }
            }
            if cropped.is_none() {
                report.push(format!("Façade {stem} copiée en entier (découpe impossible{})", if dll.is_none() { " : CodeWalker introuvable" } else { "" }));
                let dest = stream.join(format!("{new}.{ext}"));
                if std::fs::copy(p, &dest).is_err() {
                    continue;
                }
                // la copie garde la date du fichier d'origine : date du jour, pour que le nettoyage des anciens
                // binaires de l'export (export_project) la reconnaisse comme écrite maintenant
                if let Ok(f) = std::fs::File::options().write(true).open(&dest) {
                    let _ = f.set_modified(std::time::SystemTime::now());
                }
            }
            let pos = mv.point(e.pos);
            let rot = mv.q.mul(e.rot()).normalized();
            let mut lod = e.lod_dist().max(arch.lod_dist).max(150.0);
            // version de loin du jeu (parent LOD du placement d'origine)
            let mut parent_index = -1i32;
            let mut lod_level = "LODTYPES_DEPTH_ORPHANHD";
            if let Some(pi) = w.lod_parent(ei) {
                let pe = &w.ents[pi];
                let lname = w.name_of(pe.arch);
                if pe.nchild == 1 && kept_ratio >= LOD_KEEP_MIN {
                    // les niveaux au-dessus (SLOD) sont communs au quartier : pas copiés ; la version de loin reste
                    // affichée jusqu'à la distance du plus haut d'entre eux (3 000 m au plus), comme lods.rs
                    let mut far = pe.lod_dist();
                    let mut up = w.lod_parent(pi);
                    let mut guard = 0;
                    while let Some(q) = up {
                        far = far.max(w.ents[q].lod_dist());
                        up = w.lod_parent(q);
                        guard += 1;
                        if guard > 6 {
                            break;
                        }
                    }
                    let far = far.min(3000.0);
                    let lp = mv.point(pe.pos);
                    let lq = mv.q.mul(pe.rot()).normalized();
                    let lfq = lq.inverse().normalized();
                    lod_ents += &format!(
                        "  <Item type=\"CEntityDef\">\n   <archetypeName>{lname}</archetypeName>\n   <flags value=\"{}\" />\n   <guid value=\"0\" />\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <rotation x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n   <scaleXY value=\"{}\" />\n   <scaleZ value=\"{}\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"{far}\" />\n   <childLodDist value=\"{}\" />\n   <lodLevel>LODTYPES_DEPTH_LOD</lodLevel>\n   <numChildren value=\"1\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
                        crate::mech::ENTITY_FLAGS,
                        lp.x, lp.y, lp.z, lfq.x, lfq.y, lfq.z, lfq.w, pe.sxy(), pe.sz(),
                        if pe.child_lod_dist() > 0.0 { pe.child_lod_dist() } else { e.lod_dist() }
                    );
                    let (amn, amx) = w.arch_box(pe.arch);
                    let ls = v3(pe.sxy(), pe.sxy(), pe.sz());
                    for i in 0..8 {
                        let k = v3(if i & 1 == 0 { amn.x } else { amx.x }, if i & 2 == 0 { amn.y } else { amx.y }, if i & 4 == 0 { amn.z } else { amx.z });
                        let wp = lp + lq.rotate(k.mulv(ls));
                        lmn = lmn.min(wp);
                        lmx = lmx.max(wp);
                    }
                    lod_far = lod_far.max(far);
                    if let Some(pa) = w.archetypes.get(&pe.arch) {
                        let yt = w.vfs.files[pa.ytyp_file].name.trim_end_matches(".ytyp").to_string();
                        if !lod_ytyps.contains(&yt) {
                            lod_ytyps.push(yt);
                        }
                    }
                    parent_index = n_lod;
                    n_lod += 1;
                    lod_level = "LODTYPES_DEPTH_HD";
                    // la façade laisse la place à la version de loin à sa distance d'origine
                    lod = if e.lod_dist() > 0.0 { e.lod_dist() } else { lod };
                    lod_notes.push(format!("{stem} → {lname} (jusqu'à {far:.0} m)"));
                } else {
                    // version de loin commune à plusieurs bâtiments (elle ferait revenir les voisins) ou façade très
                    // découpée : la façade reste affichée jusqu'à la distance de ce parent
                    lod = lod.max(pe.lod_dist().min(1500.0));
                    lod_notes.push(format!("{stem} visible jusqu'à {lod:.0} m (version de loin du jeu {})", if pe.nchild != 1 { "commune à plusieurs bâtiments" } else { "trop différente de la façade découpée" }));
                }
            }
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
                "  <Item type=\"CEntityDef\">\n   <archetypeName>{new}</archetypeName>\n   <flags value=\"{}\" />\n   <guid value=\"0\" />\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <rotation x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n   <scaleXY value=\"{}\" />\n   <scaleZ value=\"{}\" />\n   <parentIndex value=\"{parent_index}\" />\n   <lodDist value=\"{lod}\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>{lod_level}</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
                // parent dans la carte parente : bit 0x8 (LOD_IN_PARENT_YMAP)
                lod_in_parent_flags(crate::mech::ENTITY_FLAGS, parent_index >= 0),
                pos.x, pos.y, pos.z, fq.x, fq.y, fq.z, fq.w, e.sxy(), e.sz()
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
        let l = v3(lod_max, lod_max, lod_max);
        let lod_name = format!("{ytyp_name}_lod");
        // carte des façades (HD) : parente = carte des versions de loin s'il y en a (CMapData.parent), comme lods.rs ;
        // drapeaux comme YmapFile::CalcFlags (model.rs calc_flags) : HD → contentFlags 1, LOD → flags 2 et
        // contentFlags 2
        let parent = if n_lod > 0 { lod_name.as_str() } else { "" };
        let _ = std::fs::write(stream.join(format!("{ytyp_name}.ymap.xml")), raw_ymap(&ytyp_name, parent, 0, 1, (emn - l, emx + l), (emn, emx), &ents));
        if n_lod > 0 {
            let ll = v3(lod_far, lod_far, lod_far);
            let _ = std::fs::write(stream.join(format!("{lod_name}.ymap.xml")), raw_ymap(&lod_name, "", 2, 2, (lmn - ll, lmx + ll), (lmn, lmx), &lod_ents));
            // le .ytyp du jeu qui définit chaque version de loin doit être chargé avec cette carte : dépendance
            // ajoutée au _manifest.ymf de l'export (comme EditProjectManifestPanel::GenerateProjectManifest
            // 0x06001194 : chaque carte déclare les .ytyp de ses objets)
            let ok = add_manifest_imap(stream, &lod_name, &lod_ytyps) && add_manifest_imap(stream, &ytyp_name, &[ytyp_name.clone()]);
            if !ok {
                report.push(format!("Version de loin du MLO « {} » : pas de _manifest.ymf de DragoWalker où déclarer {} — elle peut ne pas s'afficher.", r.name, lod_ytyps.join(", ")));
            }
        }
        if !lod_notes.is_empty() {
            report.push(format!("Vue de loin du MLO « {} » : {}.", r.name, lod_notes.join(" ; ")));
        }
        report.push(format!(
            "Façade du MLO « {} » copiée à côté de l'intérieur déplacé ({}) sous un nouveau nom, avec une collision générée ; le bâtiment du jeu reste intact à l'endroit d'origine.",
            r.name,
            done.join(", ")
        ));
        (report, format!("data_file 'DLC_ITYP_REQUEST' 'stream/{ytyp_name}.ytyp'\n"))
    }

    /// Pièces du MLO ouvert dans la fenêtre, gardées en mémoire (reconnaître une pièce du jeu parcourt toute la liste
    /// des fichiers du jeu) ; avec le placement du jeu de chaque façade / route.
    fn pieces_cached(&mut self, r: &MloRes) -> Vec<Piece> {
        let game = self.game.world.is_some();
        if self.mlo_state.pieces_cache.0 != r.name || self.mlo_state.pieces_cache.2 != game {
            let pieces = self.mlo_pieces(r);
            let places = pieces.iter().filter(|p| matches!(p.kind, PieceKind::Facade | PieceKind::Road)).map(|p| (p.file.clone(), self.mlo_game_placement(r, &p.stem))).collect();
            self.mlo_state.pieces_cache = (r.name.clone(), pieces, game);
            self.mlo_state.place_cache = (r.name.clone(), places);
        }
        self.mlo_state.pieces_cache.1.clone()
    }

    /// Placement du jeu d'une façade / route (en mémoire si possible).
    fn cached_placement(&self, r: &MloRes, p: &Piece) -> Option<usize> {
        if self.mlo_state.place_cache.0 == r.name {
            if let Some(x) = self.mlo_state.place_cache.1.iter().find(|x| x.0 == p.file) {
                return x.1;
            }
        }
        self.mlo_game_placement(r, &p.stem)
    }

    /// Boîtes d'une pièce là où elle sera à l'export (façade / route : déplacée avec le MLO).
    fn piece_now_boxes(&self, r: &MloRes, p: &Piece) -> Vec<[V3; 8]> {
        let moved = !self.mlo_move(r).is_identity();
        let all = self.piece_boxes(r, p);
        let n = all.len();
        all.into_iter().filter(|(_, at_origin)| !(moved && *at_origin && n > 1)).map(|(b, _)| b).collect()
    }

    /// Aperçu de l'export : façades et routes du jeu retouchées par le créateur (modèle du créateur), cochées, là
    /// où elles seront posées. Seulement quand le MLO est vu seul dans la fenêtre « Pièces du MLO ».
    pub fn mlo_preview_insts(&self) -> Vec<(u32, V3, Quat, f32, f32)> {
        let mut out = Vec::new();
        let (Some(name), Some(w)) = (self.mlo_state.pieces_open.as_ref(), self.game.world.as_deref()) else { return out };
        if !self.mlo_state.solo || self.mlo_state.pieces_cache.0 != *name {
            return out;
        }
        let Some(r) = self.proj.mlo_res.iter().find(|r| &r.name == name) else { return out };
        let mv = self.mlo_move(r);
        let moved = !mv.is_identity();
        for p in self.mlo_state.pieces_cache.1.iter().filter(|p| matches!(p.kind, PieceKind::Facade | PieceKind::Road) && r.taken(p, moved)) {
            let Some(ei) = self.cached_placement(r, p) else { continue };
            let e = &w.ents[ei];
            out.push((e.arch, mv.point(e.pos), mv.q.mul(e.rot()).normalized(), e.sxy(), e.sz()));
        }
        out
    }

    /// Objets du projet montrés quand le MLO est vu seul : ses cartes et son intérieur, sans les modèles décochés.
    fn mlo_solo_ids(&self, r: &MloRes, pieces: &[Piece], moved: bool) -> std::collections::HashSet<u64> {
        let left: Vec<String> = pieces.iter().filter(|p| p.kind == PieceKind::Model && !r.taken(p, moved)).map(|p| p.stem.to_lowercase()).collect();
        let ytyps: Vec<usize> = self.proj.ytyps.iter().enumerate().filter(|(_, t)| r.ytyp_files.iter().any(|f| f.0.eq_ignore_ascii_case(&t.name))).map(|(i, _)| i).collect();
        let mut ids = std::collections::HashSet::new();
        for it in self.proj.all_items() {
            let mine = match it {
                crate::model::ItemRef::Ent(y, _) => r.ymaps.contains(&self.proj.ymaps[y].name),
                crate::model::ItemRef::MloEnt(t, _, _) => ytyps.contains(&t),
            };
            if !mine {
                continue;
            }
            let Some(e) = self.proj.ent(it) else { continue };
            if !left.contains(&e.archetype.to_lowercase()) {
                ids.insert(e.id);
            }
        }
        ids
    }

    /// Fenêtre « Pièces du MLO » : chaque morceau de la ressource avec une case « je prends ». Le MLO est vu seul,
    /// tel qu'il sera exporté (façades du jeu déplacées avec lui, pièces décochées en rouge) ; une pièce survolée
    /// dans la liste ou dans la vue est surlignée et nommée.
    pub fn mlo_pieces_window(&mut self, ctx: &egui::Context) {
        let Some(name) = self.mlo_state.pieces_open.clone() else { return };
        let Some(ri) = self.proj.mlo_res.iter().position(|r| r.name == name) else {
            self.mlo_state.pieces_open = None;
            return;
        };
        let r = self.proj.mlo_res[ri].clone();
        let moved = !self.mlo_move(&r).is_identity();
        let pieces = self.pieces_cached(&r);
        let game = self.game.world.is_some();
        let mut open = true;
        let mut changes: Vec<(String, bool)> = Vec::new();
        let mut reset = false;
        let mut hovered: Option<usize> = None;
        let mut goto: Option<V3> = None;
        let mut ran = false;
        let mut solo = self.mlo_state.solo;
        let view_hover = self.mlo_state.view_hover.clone();
        let focus = self.mlo_state.focus.clone();
        // une seule fenêtre par image (les panneaux passent deux fois : show_panel ne la montre qu'une fois)
        self.show_panel(ctx, "mlo_pieces", &format!("Pièces du MLO « {name} »"), crate::docking::Dock::Floating, [440.0, 600.0], &mut open, |s, ui| {
            ran = true;
            ui.label(RichText::new(if moved {
                "MLO déplacé : les pièces cochées sont emportées au nouvel endroit (façades et routes du jeu copiées sous un nouveau nom et découpées autour de l'intérieur)."
            } else {
                "MLO à sa place : les pièces cochées sont mises telles quelles, comme dans la ressource du créateur ; une pièce du jeu décochée laisse le jeu d'origine."
            }).small());
            if !game {
                ui.label(RichText::new("Charge le jeu (onglet Jeu) pour reconnaître les façades et les voir.").small().color(Color32::from_rgb(255, 160, 110)));
            }
            ui.checkbox(&mut solo, "Voir le MLO seul, tel qu'il sera exporté").on_hover_text("Cache la carte du jeu et le reste du projet ; les façades du jeu sont montrées là où elles seront posées, les pièces retirées en rouge");
            ui.label(RichText::new("Souris sur une pièce dans la vue : son nom s'affiche · touche X : la retirer / la remettre · clic : la retrouver ici. Survol d'une ligne : orange = endroit d'origine, vert = là où elle sera posée.").small().weak());
            ui.separator();
            egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                let mut last: Option<PieceKind> = None;
                for (i, p) in pieces.iter().enumerate() {
                    if last != Some(p.kind) {
                        ui.add_space(4.0);
                        ui.label(RichText::new(p.kind.label()).strong().color(Color32::from_rgb(80, 220, 255)));
                        last = Some(p.kind);
                    }
                    let mut v = r.taken(p, moved);
                    let lit = view_hover.as_deref() == Some(p.file.as_str()) || focus.as_deref() == Some(p.file.as_str());
                    let resp = ui
                        .horizontal(|ui| {
                            let text = if lit { RichText::new(&p.file).color(Color32::from_rgb(255, 220, 80)).strong() } else { RichText::new(&p.file) };
                            let c = ui.checkbox(&mut v, text);
                            let tip = match p.kind {
                                PieceKind::Facade => "Bâtiment du jeu retouché par le créateur (la devanture du MLO).",
                                PieceKind::Road => "Morceau de route du jeu retouché : à un autre endroit, il amène un bout de route.",
                                PieceKind::GameCol => "Collision du quartier d'origine : déplacée, elle peut laisser des murs invisibles ; la façade a déjà sa collision générée.",
                                PieceKind::InteriorCol => "Sans elle, on traverse les murs et le sol de l'intérieur.",
                                PieceKind::Col => "Collision ajoutée par le créateur (déplacée avec le MLO).",
                                PieceKind::Model => "Modèle du créateur (coque de l'intérieur, portes, vitres…).",
                            };
                            let c = c.on_hover_text(tip);
                            if c.changed() {
                                changes.push((p.file.clone(), v));
                            }
                            ui.label(if v { RichText::new("pris").small().color(Color32::from_rgb(120, 220, 120)) } else { RichText::new("retiré").small().color(Color32::from_rgb(255, 110, 90)) });
                            let go = matches!(p.kind, PieceKind::Facade | PieceKind::Road | PieceKind::Model | PieceKind::InteriorCol);
                            if go && ui.small_button("Voir").clicked() {
                                if let Some(b) = s.piece_now_boxes(&r, p).first() {
                                    goto = Some(b.iter().fold(V3::ZERO, |a, c| a + *c) * 0.125);
                                }
                            }
                            if v != piece_default(p.kind, moved) {
                                ui.label(RichText::new("(changé)").small().weak());
                            }
                        })
                        .response;
                    if focus.as_deref() == Some(p.file.as_str()) {
                        resp.scroll_to_me(Some(egui::Align::Center));
                    }
                    if ui.rect_contains_pointer(resp.rect) {
                        hovered = Some(i);
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Choix par défaut").on_hover_text(if moved { "Tout pris sauf les routes et les collisions du quartier d'origine" } else { "Tout pris, comme le créateur" }).clicked() {
                    reset = true;
                }
                ui.label(RichText::new("Appliqué au prochain export (et au prochain test en jeu).").small().weak());
            });
        });
        if !ran && open {
            return;
        }
        self.mlo_state.focus = None;
        // surlignage dans la vue (lu par view.rs pendant cette image)
        let frame = ctx.cumulative_pass_nr();
        self.mlo_state.hl = (frame, hovered.map(|i| self.piece_boxes(&r, &pieces[i])).unwrap_or_default());
        if !changes.is_empty() || reset {
            self.push_undo();
            let m = &mut self.proj.mlo_res[ri];
            if reset {
                m.picks.clear();
            }
            for (f, v) in changes {
                m.set_pick(&f, v);
            }
        }
        if let Some(p) = goto {
            self.goto(p, 50.0);
        }
        // « Tout revoir » du bandeau ou Échap : la vue seule est arrêtée
        if self.mlo_state.solo_owned && self.isolate_sel.is_none() {
            self.mlo_state.solo_owned = false;
            solo = false;
        }
        self.mlo_state.solo = solo;
        if !open {
            self.mlo_state.pieces_open = None;
            self.mlo_state.hl.1.clear();
            self.mlo_state.view_hover = None;
        }
        if open && solo {
            let r = self.proj.mlo_res[ri].clone();
            self.isolate_sel = Some(self.mlo_solo_ids(&r, &pieces, moved));
            self.mlo_state.solo_owned = true;
        } else if self.mlo_state.solo_owned {
            self.isolate_sel = None;
            self.mlo_state.solo_owned = false;
        }
    }

    /// Dans la vue, fenêtre « Pièces du MLO » ouverte : pièce survolée dans la liste (orange / vert), pièces retirées
    /// (rouge), pièce sous la souris nommée (jaune) ; touche X : la retirer / la remettre ; clic : la montrer
    /// dans la liste.
    pub fn mlo_pieces_view(&mut self, painter: &egui::Painter, hover_pos: Option<egui::Pos2>, ctx: &egui::Context) {
        let (f, boxes) = &self.mlo_state.hl;
        if !boxes.is_empty() && ctx.cumulative_pass_nr().saturating_sub(*f) <= 1 {
            for (b, at_origin) in boxes {
                let col = if *at_origin { Color32::from_rgb(255, 150, 40) } else { Color32::from_rgb(90, 255, 120) };
                crate::render::draw_box_edges(painter, &self.cam, b, egui::Stroke::new(2.5_f32, col));
            }
        }
        self.mlo_state.view_hover = None;
        let Some(name) = self.mlo_state.pieces_open.clone() else { return };
        let Some(ri) = self.proj.mlo_res.iter().position(|r| r.name == name) else { return };
        if self.mlo_state.pieces_cache.0 != name {
            return;
        }
        let r = self.proj.mlo_res[ri].clone();
        let moved = !self.mlo_move(&r).is_identity();
        let pieces = self.mlo_state.pieces_cache.1.clone();
        let visible = |p: &Piece| matches!(p.kind, PieceKind::Facade | PieceKind::Road | PieceKind::Model);
        // pièces retirées : en rouge, là où elles auraient été
        for p in pieces.iter().filter(|p| visible(p) && !r.taken(p, moved)) {
            for b in self.piece_now_boxes(&r, p) {
                crate::render::draw_box_edges(painter, &self.cam, &b, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 80, 70)));
            }
        }
        // mode test : DW_TEST_PIECE_HOVER="x,y" fait comme si la souris était là
        let test_hp = std::env::var("DW_TEST_PIECE_HOVER").ok().and_then(|v| v.split_once(',').and_then(|(a, b)| Some(egui::pos2(a.trim().parse().ok()?, b.trim().parse().ok()?))));
        let Some(hp) = test_hp.or(hover_pos) else { return };
        if test_hp.is_none() && ctx.is_pointer_over_area() {
            return;
        }
        // pièce sous la souris : comme CodeWalker WorldForm::UpdateMouseHits(DrawableBase, Archetype, YmapEntityDef)
        // 0x060002ab (IL_05b6–IL_05d4), parmi les boîtes touchées par le rayon (distance > 0), celle dont la
        // demi-diagonale |max − min| × échelle × 0,5 est la plus petite l'emporte (une porte plutôt que la coque qui
        // la contient) ; à taille égale, la plus proche
        let (o, d) = self.cam.ray(hp);
        let mut best: Option<(f32, f32, usize, [V3; 8])> = None;
        for (i, p) in pieces.iter().enumerate().filter(|(_, p)| visible(p)) {
            for b in self.piece_now_boxes(&r, p) {
                let Some(t) = ray_obb(&b, o, d) else { continue };
                let half_diag = (b[7] - b[0]).len() * crate::mech::PICK_HALF_DIAG;
                if best.map(|x| (half_diag, t) < (x.0, x.1)).unwrap_or(true) {
                    best = Some((half_diag, t, i, b));
                }
            }
        }
        let Some((_, _, i, b)) = best else { return };
        let p = &pieces[i];
        let taken = r.taken(p, moved);
        crate::render::draw_box_edges(painter, &self.cam, &b, egui::Stroke::new(3.0_f32, Color32::from_rgb(255, 220, 60)));
        let text = format!("{}\n{} · {}\nX : {} · clic : la retrouver dans la liste", p.file, p.kind.label(), if taken { "prise à l'export" } else { "retirée de l'export" }, if taken { "la retirer" } else { "la remettre" });
        let galley = painter.layout(text, egui::FontId::proportional(13.0), Color32::WHITE, 420.0);
        let at = hp + egui::vec2(18.0, 14.0);
        painter.rect_filled(egui::Rect::from_min_size(at - egui::vec2(6.0, 4.0), galley.size() + egui::vec2(12.0, 8.0)), 5.0, Color32::from_rgba_unmultiplied(20, 20, 24, 235));
        painter.galley(at, galley, Color32::WHITE);
        self.mlo_state.view_hover = Some(p.file.clone());
        let (x, click) = ctx.input(|inp| (inp.key_pressed(egui::Key::X) && !inp.modifiers.command, inp.pointer.primary_clicked()));
        if x && !ctx.wants_keyboard_input() {
            self.push_undo();
            self.proj.mlo_res[ri].set_pick(&p.file, !taken);
            self.status = format!("{} {} de l'export du MLO « {name} ».", p.file, if taken { "retirée" } else { "remise" });
        }
        if click {
            self.mlo_state.focus = Some(p.file.clone());
        }
    }

    /// Liste des MLO importés (panneau Projet).
    pub fn mlo_list_ui(&mut self, ui: &mut egui::Ui) {
        if self.proj.mlo_res.is_empty() {
            return;
        }
        let mut select: Option<Vec<u64>> = None;
        let mut remove = None;
        let mut goto = None;
        let mut open_pieces = None;
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
                    if ui.small_button("Pièces…").on_hover_text("Voir le MLO seul, tel qu'il sera exporté, et choisir ses pièces (intérieur, façades, route, collisions)").clicked() {
                        open_pieces = Some(r.name.clone());
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
        if open_pieces.is_some() {
            self.mlo_state.pieces_open = open_pieces;
            self.mlo_state.solo = true;
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
