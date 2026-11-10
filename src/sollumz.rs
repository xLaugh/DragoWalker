//! Pont avec Blender + Sollumz : DragoWalker lit les modèles que Sollumz exporte au format XML de
//! CodeWalker (.ydr.xml, textures .dds, .ytyp.xml) et peut lui renvoyer des morceaux de carte (.ymap.xml).
//! Formats suivis : VertexBuffer::WriteXml 0x06004a66 (Layout + Data), VertexData::WriteXml 0x06004a7d
//! (composants séparés par 3 espaces, valeurs par 1 espace), IndexBuffer::WriteXml 0x06004ad8,
//! DrawableGeometry::WriteXml 0x06004a41, ShaderFX::WriteXml 0x06004849, ShaderParametersBlock::WriteXml 0x0600487f.

use crate::gta::crypto::joaat;
use crate::gta::drawable::{Mesh, RGeom, RModel, RVert, TexData, TexFmt};
use crate::math::{v3, V3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Un modèle personnel exporté depuis Blender.
pub struct CustomModel {
    pub name: String,
    pub file: PathBuf,
    pub model: RModel,
    /// textures trouvées (clé, données)
    pub textures: Vec<(u64, TexData)>,
    pub part_keys: Vec<u64>,
    pub bb_min: V3,
    pub bb_max: V3,
    pub tris: usize,
    pub missing_textures: Vec<String>,
}

pub struct SollumzState {
    pub dir: String,
    pub models: Vec<CustomModel>,
    pub archetypes: HashMap<String, (V3, V3)>,
    pub errors: Vec<String>,
    pub blender: Option<PathBuf>,
    pub sollumz: Option<String>,
    stamps: HashMap<PathBuf, SystemTime>,
    pub last_scan: f64,
    /// version incrémentée à chaque rechargement (pour renvoyer les modèles à la carte graphique)
    pub generation: u64,
}

impl SollumzState {
    pub fn new() -> SollumzState {
        let s = crate::app::load_settings();
        let dir = std::env::var("DW_SOLLUMZ_DIR").ok().or_else(|| s["sollumz_dir"].as_str().map(|x| x.to_string())).unwrap_or_else(default_dir);
        SollumzState {
            dir,
            models: Vec::new(),
            archetypes: HashMap::new(),
            errors: Vec::new(),
            blender: find_blender(),
            sollumz: find_sollumz(),
            stamps: HashMap::new(),
            last_scan: -10.0,
            generation: 0,
        }
    }

    /// Relit le dossier si un fichier a changé. Rend vrai si quelque chose a été rechargé.
    pub fn refresh(&mut self, force: bool) -> bool {
        let dir = PathBuf::from(&self.dir);
        if !dir.is_dir() {
            return false;
        }
        let mut files = Vec::new();
        collect(&dir, &mut files, 0);
        let mut stamps = HashMap::new();
        for f in &files {
            if let Ok(m) = std::fs::metadata(f).and_then(|m| m.modified()) {
                stamps.insert(f.clone(), m);
            }
        }
        if !force && stamps == self.stamps {
            return false;
        }
        self.stamps = stamps;
        self.models.clear();
        self.archetypes.clear();
        self.errors.clear();
        // textures .dds du dossier, par hachage de leur nom (pour les .ydr binaires sans textures intégrées)
        let dds: HashMap<u32, PathBuf> = files
            .iter()
            .filter(|f| f.extension().map(|x| x.eq_ignore_ascii_case("dds")).unwrap_or(false))
            .flat_map(|f| {
                let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                // Sollumz peut garder l'extension dans le nom de la texture
                [(joaat(&stem), f.clone()), (joaat(&format!("{stem}.dds")), f.clone())]
            })
            .collect();
        let newest = |list: &mut Vec<CustomModel>, m: CustomModel| {
            let t = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            match list.iter().position(|x| x.name == m.name) {
                Some(i) if t(&list[i].file) > t(&m.file) => {}
                Some(i) => list[i] = m,
                None => list.push(m),
            }
        };
        let mut ci = 0u32;
        for f in &files {
            let n = f.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if n.ends_with(".ydr.xml") {
                match read_ydr_xml(f, ci, &dds) {
                    Ok(m) => {
                        newest(&mut self.models, m);
                        ci += 1;
                    }
                    Err(e) => self.errors.push(format!("{n} : {e}")),
                }
            } else if n.ends_with(".ydr") {
                // export binaire de Sollumz (réglage par défaut des versions récentes)
                match read_ydr_binary(f, ci, &dds) {
                    Ok(m) => {
                        newest(&mut self.models, m);
                        ci += 1;
                    }
                    Err(e) => self.errors.push(format!("{n} : {e}")),
                }
            } else if n.ends_with(".ytyp.xml") {
                match std::fs::read_to_string(f).map_err(|e| e.to_string()).and_then(|s| read_ytyp_boxes(&s)) {
                    Ok(list) => {
                        for (k, b) in list {
                            self.archetypes.insert(k, b);
                        }
                    }
                    Err(e) => self.errors.push(format!("{n} : {e}")),
                }
            }
        }
        self.models.sort_by(|a, b| a.name.cmp(&b.name));
        self.generation += 1;
        true
    }

    /// Taille d'un modèle personnel : celle du .ytyp.xml s'il existe, sinon celle du modèle.
    pub fn boxes(&self) -> HashMap<String, (V3, V3)> {
        let mut m: HashMap<String, (V3, V3)> = self.models.iter().map(|c| (c.name.clone(), (c.bb_min, c.bb_max))).collect();
        for (k, b) in &self.archetypes {
            m.insert(k.clone(), *b);
        }
        m
    }

    pub fn find(&self, name: &str) -> Option<&CustomModel> {
        let n = name.to_lowercase();
        self.models.iter().find(|m| m.name == n)
    }
}

pub fn default_dir() -> String {
    let base = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    base.join("Documents").join("DragoWalker").join("Sollumz").to_string_lossy().to_string()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            // Sollumz exporte aussi une version « gen9 » (GTA V Enhanced) : on garde la version classique
            let dn = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if dn == "gen9" {
                continue;
            }
            if depth < 3 {
                collect(&p, out, depth + 1);
            }
        } else {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if n.ends_with(".ydr.xml") || n.ends_with(".ytyp.xml") || n.ends_with(".dds") || n.ends_with(".ydr") || n.ends_with(".ytyp") {
                out.push(p);
            }
        }
    }
}

/// Blender installé (Program Files, Steam, ou dans le PATH).
pub fn find_blender() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    for root in ["C:\\Program Files\\Blender Foundation", "C:\\Program Files (x86)\\Blender Foundation"] {
        if let Ok(rd) = std::fs::read_dir(root) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path().join("blender.exe")).filter(|p| p.is_file()).collect();
            v.sort();
            cands.extend(v.into_iter().rev());
        }
    }
    for d in ["C", "D", "E", "F", "G"] {
        for sub in ["Program Files (x86)\\Steam\\steamapps\\common\\Blender", "SteamLibrary\\steamapps\\common\\Blender", "Steam\\steamapps\\common\\Blender"] {
            cands.push(PathBuf::from(format!("{d}:\\{sub}\\blender.exe")));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&path) {
            cands.push(p.join("blender.exe"));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Module Sollumz installé dans Blender (dossier addons ou extensions). Rend « Blender x.y ».
pub fn find_sollumz() -> Option<String> {
    let base = PathBuf::from(std::env::var_os("APPDATA")?).join("Blender Foundation").join("Blender");
    let mut vers: Vec<PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).collect();
    vers.sort();
    for v in vers.iter().rev() {
        let mut places = vec![v.join("scripts").join("addons")];
        if let Ok(rd) = std::fs::read_dir(v.join("extensions")) {
            places.extend(rd.flatten().map(|e| e.path()));
        }
        for pl in places {
            if let Ok(rd) = std::fs::read_dir(&pl) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().to_lowercase().contains("sollumz") {
                        return Some(format!("Blender {}", v.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                    }
                }
            }
        }
    }
    None
}

fn child<'a, 'b>(n: roxmltree::Node<'a, 'b>, name: &str) -> Option<roxmltree::Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(name))
}

fn vec_attr(n: Option<roxmltree::Node>) -> Option<V3> {
    let n = n?;
    let f = |k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok());
    Some(v3(f("x")?, f("y")?, f("z")?))
}

/// Lit un .ydr.xml exporté par Sollumz (ou CodeWalker) avec ses textures .dds.
pub fn read_ydr_xml(path: &Path, custom_index: u32, dds: &HashMap<u32, PathBuf>) -> Result<CustomModel, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc = roxmltree::Document::parse(&src).map_err(|e| format!("XML illisible : {e}"))?;
    let root = doc.root_element();
    let fname = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let name = child(root, "Name").and_then(|n| n.text()).map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).unwrap_or_else(|| fname.trim_end_matches(".ydr.xml").to_string());

    // matériaux : texture de couleur et découpe (RenderBucket 1, 2, 3)
    let mut shaders: Vec<(String, bool)> = Vec::new();
    if let Some(sh) = child(root, "ShaderGroup").and_then(|g| child(g, "Shaders")) {
        for it in sh.children().filter(|c| c.is_element()) {
            let bucket = child(it, "RenderBucket").and_then(|b| b.attribute("value")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            let mut tex = String::new();
            let mut first = String::new();
            if let Some(ps) = child(it, "Parameters") {
                for p in ps.children().filter(|c| c.is_element()) {
                    if p.attribute("type") != Some("Texture") {
                        continue;
                    }
                    let tn = child(p, "Name").and_then(|n| n.text()).unwrap_or("").trim().to_lowercase();
                    if tn.is_empty() {
                        continue;
                    }
                    if p.attribute("name").map(|n| n.eq_ignore_ascii_case("DiffuseSampler")).unwrap_or(false) {
                        tex = tn;
                        break;
                    }
                    if first.is_empty() && !tn.ends_with("_n") && !tn.ends_with("_s") {
                        first = tn;
                    }
                }
            }
            shaders.push((if tex.is_empty() { first } else { tex }, matches!(bucket, 1..=3)));
        }
    }

    let mut model = RModel { verts: Vec::new(), idx: Vec::new(), geoms: Vec::new(), embedded_dict: None };
    let lod = ["DrawableModelsHigh", "DrawableModelsMedium", "DrawableModelsLow", "DrawableModelsVeryLow"].iter().find_map(|t| child(root, t)).ok_or("aucun modèle (DrawableModelsHigh) dans le fichier")?;
    for m in lod.children().filter(|c| c.is_element()) {
        let Some(geos) = child(m, "Geometries") else { continue };
        for g in geos.children().filter(|c| c.is_element()) {
            let sidx = child(g, "ShaderIndex").and_then(|s| s.attribute("value")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let (tex, cutout) = shaders.get(sidx).cloned().unwrap_or_default();
            read_xml_geometry(g, &mut model, if tex.is_empty() { 0 } else { joaat(&tex) }, cutout)?;
        }
    }
    if model.idx.is_empty() {
        return Err("modèle vide".into());
    }
    // textures .dds : même dossier, dossier du nom du modèle, ou sous-dossiers
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut textures = Vec::new();
    let mut missing = Vec::new();
    let mut part_keys = Vec::new();
    let mut done: HashMap<u32, u64> = HashMap::new();
    for g in &model.geoms {
        if g.tex == 0 {
            part_keys.push(0);
            continue;
        }
        if let Some(k) = done.get(&g.tex) {
            part_keys.push(*k);
            continue;
        }
        let tname = shaders.iter().find(|s| joaat(&s.0) == g.tex).map(|s| s.0.clone()).unwrap_or_default();
        let key = ((0xC000_0000u64 | custom_index as u64) << 32) | g.tex as u64;
        let found = [dir.join(format!("{tname}.dds")), dir.join(&name).join(format!("{tname}.dds"))]
            .into_iter()
            .find(|p| p.is_file())
            .or_else(|| find_file(dir, &format!("{tname}.dds"), 0))
            .or_else(|| dds.get(&joaat(&tname)).cloned());
        match found.and_then(|p| std::fs::read(p).ok()).and_then(|d| read_dds(&d, crate::gta::loader::MAX_TEX)) {
            Some(t) => {
                textures.push((key, t));
                done.insert(g.tex, key);
                part_keys.push(key);
            }
            None => {
                missing.push(tname);
                done.insert(g.tex, 0);
                part_keys.push(0);
            }
        }
    }
    let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for v in &model.verts {
        let p = v3(v.pos[0], v.pos[1], v.pos[2]);
        mn = mn.min(p);
        mx = mx.max(p);
    }
    let tris = model.idx.len() / 3;
    Ok(CustomModel { name, file: path.to_path_buf(), model, textures, part_keys, bb_min: mn, bb_max: mx, tris, missing_textures: missing })
}

fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            subs.push(p);
        } else if p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false) {
            return Some(p);
        }
    }
    if depth < 2 {
        for s in subs {
            if let Some(p) = find_file(&s, name, depth + 1) {
                return Some(p);
            }
        }
    }
    None
}

/// Une géométrie : Layout (noms des composants) + Data (une ligne par sommet).
fn read_xml_geometry(g: roxmltree::Node, model: &mut RModel, tex: u32, cutout: bool) -> Result<(), String> {
    let vb = child(g, "VertexBuffer").ok_or("VertexBuffer manquant")?;
    let layout: Vec<String> = child(vb, "Layout").map(|l| l.children().filter(|c| c.is_element()).map(|c| c.tag_name().name().to_string()).collect()).unwrap_or_default();
    let data = child(vb, "Data").or_else(|| child(vb, "Data1")).and_then(|d| d.text()).unwrap_or("");
    let pi = layout.iter().position(|l| l == "Position").ok_or("pas de positions")?;
    let ni = layout.iter().position(|l| l == "Normal");
    let ti = layout.iter().position(|l| l == "TexCoord0");
    let ci = layout.iter().position(|l| l == "Colour0");
    let base = model.verts.len() as u32;
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // comme VertexData::ReadXml / SetString (0x06004a7e / 0x06004a80) : toutes les valeurs de la ligne
        // (séparées par espaces ou tabulations), lues dans l'ordre, autant que chaque composant en contient
        let vals: Vec<f32> = line.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).collect();
        let mut comps: Vec<Vec<f32>> = Vec::with_capacity(layout.len());
        let mut at = 0;
        for l in &layout {
            let n = component_count(l);
            comps.push(vals.get(at..(at + n).min(vals.len())).map(|s| s.to_vec()).unwrap_or_default());
            at += n;
        }
        let get = |i: usize| comps.get(i).cloned().unwrap_or_default();
        let p = get(pi);
        if p.len() < 3 {
            continue;
        }
        let mut v = RVert { pos: [p[0], p[1], p[2]], nrm: [0.0, 0.0, 1.0], uv: [0.0, 0.0], col: [255; 4] };
        if let Some(n) = ni.map(get).filter(|n| n.len() >= 3) {
            v.nrm = [n[0], n[1], n[2]];
        }
        if let Some(t) = ti.map(get).filter(|t| t.len() >= 2) {
            v.uv = [t[0], t[1]];
        }
        if let Some(c) = ci.map(get).filter(|c| c.len() >= 4) {
            v.col = [c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8];
        }
        model.verts.push(v);
    }
    let n = model.verts.len() as u32 - base;
    let first = model.idx.len() as u32;
    if let Some(ib) = child(g, "IndexBuffer").and_then(|i| child(i, "Data")).and_then(|d| d.text()) {
        let idx: Vec<u32> = ib.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        for t in idx.chunks_exact(3) {
            if t.iter().all(|&i| i < n) {
                model.idx.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            }
        }
    }
    let count = model.idx.len() as u32 - first;
    if count > 0 {
        model.geoms.push(RGeom { first, count, tex, cutout, blend: 0, glass: false });
    }
    Ok(())
}

/// Texture .dds (DXT1/3/5, BC7, ou non compressée 32 bits), niveaux de `max` pixels au plus.
pub fn read_dds(d: &[u8], max: u32) -> Option<TexData> {
    if d.len() < 128 || &d[0..4] != b"DDS " {
        return None;
    }
    let u = |p: usize| u32::from_le_bytes(d[p..p + 4].try_into().unwrap());
    let h = u(12);
    let w = u(16);
    let mips = u(28).max(1);
    let fourcc = &d[84..88];
    let mut off = 128;
    let fmt = match fourcc {
        b"DXT1" => TexFmt::Dxt1,
        b"DXT3" => TexFmt::Dxt3,
        b"DXT5" => TexFmt::Dxt5,
        b"DX10" => {
            off = 148;
            match u(128) {
                71 | 72 => TexFmt::Dxt1,
                74 | 75 => TexFmt::Dxt3,
                77 | 78 => TexFmt::Dxt5,
                98 | 99 => TexFmt::Bc7,
                87 | 28 | 29 => TexFmt::Rgba8,
                _ => return None,
            }
        }
        _ if u(88) == 32 => TexFmt::Rgba8,
        _ => return None,
    };
    let bgra = fourcc != b"DX10" || u(128) == 87;
    let size_of = |lw: u32, lh: u32| -> usize {
        match fmt {
            TexFmt::Dxt1 => (lw.div_ceil(4).max(1) * lh.div_ceil(4).max(1) * 8) as usize,
            TexFmt::Rgba8 => (lw * lh * 4) as usize,
            _ => (lw.div_ceil(4).max(1) * lh.div_ceil(4).max(1) * 16) as usize,
        }
    };
    let mut out = TexData { w: 0, h: 0, fmt, levels: Vec::new() };
    let (mut lw, mut lh) = (w, h);
    for _ in 0..mips {
        let sz = size_of(lw, lh);
        if off + sz > d.len() {
            break;
        }
        if lw.max(lh) <= max || (out.levels.is_empty() && lw.min(lh) <= 4) {
            if out.levels.is_empty() {
                out.w = lw;
                out.h = lh;
            }
            let mut lv = d[off..off + sz].to_vec();
            if fmt == TexFmt::Rgba8 && bgra {
                for px in lv.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
            }
            out.levels.push(lv);
        }
        off += sz;
        if lw == 1 && lh == 1 {
            break;
        }
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
        if fmt != TexFmt::Rgba8 && (lw < 4 || lh < 4) {
            break;
        }
    }
    // si aucun niveau n'est assez petit, garder le plus petit lu
    if out.levels.is_empty() {
        return None;
    }
    Some(out)
}

/// Types d'objets d'un .ytyp.xml : nom → (bbMin, bbMax).
pub fn read_ytyp_boxes(src: &str) -> Result<Vec<(String, (V3, V3))>, String> {
    let doc = roxmltree::Document::parse(src).map_err(|e| format!("XML illisible : {e}"))?;
    let mut out = Vec::new();
    for it in doc.descendants().filter(|n| n.has_tag_name("Item") && n.attribute("type").map(|t| t.ends_with("ArchetypeDef")).unwrap_or(false)) {
        let Some(name) = child(it, "name").and_then(|n| n.text()) else { continue };
        if let (Some(a), Some(b)) = (vec_attr(child(it, "bbMin")), vec_attr(child(it, "bbMax"))) {
            out.push((name.trim().to_lowercase(), (a, b)));
        }
    }
    Ok(out)
}

/// Aperçu (positions + triangles) d'un modèle personnel.
pub fn preview_mesh(m: &CustomModel) -> Mesh {
    let pos: Vec<[f32; 3]> = m.model.verts.iter().map(|v| v.pos).collect();
    let tris: Vec<[u32; 3]> = m.model.idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    Mesh { pos, tris, min: [m.bb_min.x, m.bb_min.y, m.bb_min.z], max: [m.bb_max.x, m.bb_max.y, m.bb_max.z] }
}

/// Nombre de valeurs d'un composant de sommet (VertexComponentTypes::GetComponentCount 0x06005a4c
/// appliqué aux types GTAV1 : Float3 = 3, Colour = 4, Float2 = 2, Float4 = 4).
fn component_count(semantic: &str) -> usize {
    match semantic {
        "Position" | "Normal" => 3,
        "BlendWeights" | "BlendIndices" | "Colour0" | "Colour1" | "Tangent" | "Binormal" => 4,
        s if s.starts_with("TexCoord") => 2,
        _ => 0,
    }
}

// ---------------------------------------------------------------- aller-retour avec Blender

/// En-tête DDS + niveaux d'une texture du jeu (DXT1/3/5, BC7, ou 32 bits).
pub fn write_dds(t: &TexData) -> Vec<u8> {
    let mut d = Vec::new();
    let u = |d: &mut Vec<u8>, v: u32| d.extend_from_slice(&v.to_le_bytes());
    d.extend_from_slice(b"DDS ");
    u(&mut d, 124);
    let linear = t.levels.first().map(|l| l.len()).unwrap_or(0) as u32;
    u(&mut d, 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000 | if t.fmt == TexFmt::Rgba8 { 0x8 } else { 0x80000 });
    u(&mut d, t.h);
    u(&mut d, t.w);
    u(&mut d, if t.fmt == TexFmt::Rgba8 { t.w * 4 } else { linear });
    u(&mut d, 0);
    u(&mut d, t.levels.len() as u32);
    d.extend_from_slice(&[0u8; 44]);
    u(&mut d, 32);
    match t.fmt {
        TexFmt::Rgba8 => {
            u(&mut d, 0x41);
            u(&mut d, 0);
            u(&mut d, 32);
            for m in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
                u(&mut d, m);
            }
        }
        _ => {
            u(&mut d, 0x4);
            d.extend_from_slice(match t.fmt {
                TexFmt::Dxt1 => b"DXT1",
                TexFmt::Dxt3 => b"DXT3",
                TexFmt::Dxt5 => b"DXT5",
                _ => b"DX10",
            });
            for _ in 0..5 {
                u(&mut d, 0);
            }
        }
    }
    u(&mut d, 0x1000 | if t.levels.len() > 1 { 0x400008 } else { 0 });
    for _ in 0..4 {
        u(&mut d, 0);
    }
    if t.fmt == TexFmt::Bc7 {
        // en-tête DX10 : DXGI_FORMAT_BC7_UNORM (98), texture 2D
        for v in [98u32, 3, 0, 1, 0] {
            u(&mut d, v);
        }
    }
    for l in &t.levels {
        if t.fmt == TexFmt::Rgba8 {
            // RGBA → BGRA
            for px in l.chunks_exact(4) {
                d.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        } else {
            d.extend_from_slice(l);
        }
    }
    d
}

/// Écrit un modèle au format XML de CodeWalker (lu par Sollumz) et ses textures .dds dans `dir/<nom>/`.
pub fn write_ydr_xml(dir: &Path, name: &str, model: &RModel, part_keys: &[u64], textures: &[(u64, TexData)]) -> Result<PathBuf, String> {
    write_ydr_xml_ex(dir, name, model, part_keys, textures, None)
}

/// Nom d'une texture dans un dictionnaire partagé (.ytd) : `<dictionnaire>_t<clé>`.
pub fn shared_tex_name(txd: &str, key: u64) -> String {
    format!("{}_t{:08x}", txd, key as u32)
}

/// Comme `write_ydr_xml` ; avec `shared_txd`, les textures ne sont pas écrites à côté du modèle : le modèle
/// les cherche par leur nom dans ce dictionnaire partagé (.ytd), déclaré dans le type d'objet (ytyp).
pub fn write_ydr_xml_ex(dir: &Path, name: &str, model: &RModel, part_keys: &[u64], textures: &[(u64, TexData)], shared_txd: Option<&str>) -> Result<PathBuf, String> {
    use std::fmt::Write;
    let prefix = shared_txd.unwrap_or(name).to_string();
    let tex_name = |k: u64| shared_tex_name(&prefix, k);
    if shared_txd.is_none() {
        let tex_dir = dir.join(name);
        std::fs::create_dir_all(&tex_dir).map_err(|e| e.to_string())?;
        for (k, t) in textures {
            std::fs::write(tex_dir.join(format!("{}.dds", tex_name(*k))), write_dds(t)).map_err(|e| e.to_string())?;
        }
    }
    // un matériau par couple (texture, découpe)
    let mut shaders: Vec<(u64, bool)> = Vec::new();
    let mut shader_of = Vec::new();
    for (g, k) in model.geoms.iter().zip(part_keys.iter()) {
        let key = (if textures.iter().any(|t| t.0 == *k) { *k } else { 0 }, g.cutout);
        let i = shaders.iter().position(|s| *s == key).unwrap_or_else(|| {
            shaders.push(key);
            shaders.len() - 1
        });
        shader_of.push(i);
    }
    let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &model.verts {
        for k in 0..3 {
            mn[k] = mn[k].min(v.pos[k]);
            mx[k] = mx[k].max(v.pos[k]);
        }
    }
    let c = [(mn[0] + mx[0]) * 0.5, (mn[1] + mx[1]) * 0.5, (mn[2] + mx[2]) * 0.5];
    let r = ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2) + (mx[2] - mn[2]).powi(2)).sqrt() * 0.5;
    let mut s = String::new();
    let _ = writeln!(s, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Drawable>\n <Name>{name}</Name>");
    let _ = writeln!(s, " <BoundingSphereCenter x=\"{}\" y=\"{}\" z=\"{}\" />\n <BoundingSphereRadius value=\"{r}\" />", c[0], c[1], c[2]);
    let _ = writeln!(s, " <BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" />\n <BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" />", mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]);
    let _ = writeln!(s, " <LodDistHigh value=\"9998\" />\n <LodDistMed value=\"9998\" />\n <LodDistLow value=\"9998\" />\n <LodDistVlow value=\"9998\" />");
    let _ = writeln!(s, " <FlagsHigh value=\"1\" />\n <FlagsMed value=\"0\" />\n <FlagsLow value=\"0\" />\n <FlagsVlow value=\"0\" />");
    let _ = writeln!(s, " <ShaderGroup>\n  <TextureDictionary />\n  <Shaders>");
    for (k, cut) in &shaders {
        let sh = if *cut { "cutout" } else { "default" };
        let _ = writeln!(s, "   <Item>\n    <Name>{sh}</Name>\n    <FileName>{sh}.sps</FileName>\n    <RenderBucket value=\"{}\" />\n    <Parameters>", if *cut { 3 } else { 0 });
        if *k != 0 {
            let _ = writeln!(s, "     <Item name=\"DiffuseSampler\" type=\"Texture\">\n      <Name>{}</Name>\n     </Item>", tex_name(*k));
        }
        let _ = writeln!(s, "    </Parameters>\n   </Item>");
    }
    let _ = writeln!(s, "  </Shaders>\n </ShaderGroup>\n <DrawableModelsHigh>\n  <Item>\n   <RenderMask value=\"255\" />\n   <Flags value=\"0\" />\n   <HasSkin value=\"0\" />\n   <BoneIndex value=\"0\" />\n   <Unknown1 value=\"0\" />\n   <Geometries>");
    for (gi, g) in model.geoms.iter().enumerate() {
        // sommets utilisés par cette partie, renumérotés
        let idx = &model.idx[g.first as usize..(g.first + g.count) as usize];
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut order = Vec::new();
        for &i in idx {
            remap.entry(i).or_insert_with(|| {
                order.push(i);
                order.len() as u32 - 1
            });
        }
        // boîte de la partie : CodeWalker la lit telle quelle (DrawableGeometry::ReadXml 0x06004a42) et en fait les
        // boîtes du modèle (DrawableModel::ReadXml 0x06004a17, BoundsData) ; absente, elle vaut 0 et le jeu
        // cache la partie dès que ce point sort de l'écran (terrain qui disparaît selon où l'on regarde)
        let (mut gmn, mut gmx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &i in &order {
            let p = model.verts[i as usize].pos;
            for k in 0..3 {
                gmn[k] = gmn[k].min(p[k]);
                gmx[k] = gmx[k].max(p[k]);
            }
        }
        if order.is_empty() {
            gmn = [0.0; 3];
            gmx = [0.0; 3];
        }
        let _ = writeln!(
            s,
            "    <Item>\n     <ShaderIndex value=\"{}\" />\n     <BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" />\n     <BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" />",
            shader_of[gi], gmn[0], gmn[1], gmn[2], gmx[0], gmx[1], gmx[2]
        );
        let _ = writeln!(s, "     <VertexBuffer>\n      <Flags value=\"0\" />\n      <Layout type=\"GTAV1\">\n       <Position />\n       <Normal />\n       <Colour0 />\n       <TexCoord0 />\n      </Layout>\n      <Data>");
        for &i in &order {
            let v = &model.verts[i as usize];
            let _ = writeln!(s, "       {} {} {}   {} {} {}   {} {} {} {}   {} {}", v.pos[0], v.pos[1], v.pos[2], v.nrm[0], v.nrm[1], v.nrm[2], v.col[0], v.col[1], v.col[2], v.col[3], v.uv[0], v.uv[1]);
        }
        let _ = writeln!(s, "      </Data>\n     </VertexBuffer>\n     <IndexBuffer>\n      <Data>");
        let list: Vec<String> = idx.iter().map(|i| remap[i].to_string()).collect();
        for chunk in list.chunks(24) {
            let _ = writeln!(s, "       {}", chunk.join(" "));
        }
        let _ = writeln!(s, "      </Data>\n     </IndexBuffer>\n    </Item>");
    }
    let _ = writeln!(s, "   </Geometries>\n  </Item>\n </DrawableModelsHigh>\n</Drawable>");
    let path = dir.join(format!("{name}.ydr.xml"));
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Dictionnaire de textures (.ytd) au format XML de CodeWalker : `dir/<nom>.ytd.xml` + les .dds dans
/// `dir/<nom>/`. Les textures sont directement sous `<TextureDictionary>` : XmlYtd.GetYtd (0x060030db) passe la
/// racine à TextureDictionary.ReadXmlNode (0x06005980), dont ReadXml (0x0600597e) prend les nœuds « Item »
/// enfants, lit chacun par Texture.ReadXml (0x06005a11 : Width, Height, MipLevels, Format, puis le .dds
/// FileName dans le dossier, via DDSIO.GetTexture) et finit par BuildFromTextureList (0x06005984, tri par nom).
/// Une enveloppe `<Textures>` (comme dans dsrt_shared.ytd.xml) donne un dictionnaire vide.
pub fn write_ytd_xml(dir: &Path, txd: &str, textures: &[(u64, TexData)]) -> Result<PathBuf, String> {
    use std::fmt::Write;
    let tex_dir = dir.join(txd);
    std::fs::create_dir_all(&tex_dir).map_err(|e| e.to_string())?;
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<TextureDictionary>\n");
    for (k, t) in textures {
        let n = shared_tex_name(txd, *k);
        std::fs::write(tex_dir.join(format!("{n}.dds")), write_dds(t)).map_err(|e| e.to_string())?;
        let fmt = match t.fmt {
            TexFmt::Dxt1 => "D3DFMT_DXT1",
            TexFmt::Dxt3 => "D3DFMT_DXT3",
            TexFmt::Dxt5 => "D3DFMT_DXT5",
            _ => "D3DFMT_A8R8G8B8",
        };
        let _ = writeln!(
            s,
            "  <Item>\n   <Name>{n}</Name>\n   <Unk32 value=\"0\" />\n   <Usage>DIFFUSE</Usage>\n   <UsageFlags />\n   <ExtraFlags value=\"0\" />\n   <Width value=\"{}\" />\n   <Height value=\"{}\" />\n   <MipLevels value=\"{}\" />\n   <Format>{fmt}</Format>\n   <FileName>{n}.dds</FileName>\n  </Item>",
            t.w,
            t.h,
            t.levels.len()
        );
    }
    s.push_str("</TextureDictionary>\n");
    let path = dir.join(format!("{txd}.ytd.xml"));
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Ouvre Blender et importe le fichier avec Sollumz (commande sollumz.import_assets).
pub fn open_in_blender(blender: &Path, file: &Path) -> Result<(), String> {
    let dir = file.parent().ok_or("dossier inconnu")?;
    let name = file.file_name().ok_or("fichier inconnu")?.to_string_lossy().to_string();
    let script = format!(
        "import bpy\n\
         def _dw_import():\n\
         \x20   try:\n\
         \x20       cube = bpy.data.objects.get('Cube')\n\
         \x20       if cube is not None and len(bpy.data.objects) <= 3:\n\
         \x20           bpy.data.objects.remove(cube)\n\
         \x20       bpy.ops.sollumz.import_assets(directory=r'{}\\', files=[{{'name': r'{}'}}])\n\
         \x20   except Exception as e:\n\
         \x20       print('DragoWalker : import Sollumz impossible :', e)\n\
         \x20   return None\n\
         bpy.app.timers.register(_dw_import, first_interval=1.5)\n",
        dir.display(),
        name
    );
    let sp = std::env::temp_dir().join("dragowalker_blender_import.py");
    std::fs::write(&sp, script).map_err(|e| e.to_string())?;
    std::process::Command::new(blender).arg("--python").arg(&sp).spawn().map_err(|e| format!("Impossible d'ouvrir Blender : {e}"))?;
    Ok(())
}

/// Modèle binaire (.ydr) exporté par Sollumz : lu comme ceux du jeu, avec ses textures intégrées.
pub fn read_ydr_binary(path: &Path, custom_index: u32, dds: &HashMap<u32, PathBuf>) -> Result<CustomModel, String> {
    let d = std::fs::read(path).map_err(|e| e.to_string())?;
    if d.len() < 16 || &d[0..4] != b"RSC7" {
        return Err("pas un fichier .ydr du jeu".into());
    }
    let sys = u32::from_le_bytes(d[8..12].try_into().unwrap());
    let gfx = u32::from_le_bytes(d[12..16].try_into().unwrap());
    let data = crate::gta::crypto::inflate(&d[16..]).ok_or("décompression impossible")?;
    let r = crate::gta::rpf::Resource::new(data, sys, gfx);
    let model = crate::gta::drawable::read_render_drawable(&r, 0, crate::gta::loader::MAX_TRIS).ok_or("modèle illisible")?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default().trim_end_matches(".ydr").to_string();
    let mut textures = Vec::new();
    let mut part_keys = Vec::new();
    let mut missing = Vec::new();
    for g in &model.geoms {
        let key = ((0xC000_0000u64 | custom_index as u64) << 32) | g.tex as u64;
        if g.tex == 0 {
            part_keys.push(0);
            continue;
        }
        if textures.iter().any(|(k, _)| *k == key) {
            part_keys.push(key);
            continue;
        }
        let found = model
            .embedded_dict
            .and_then(|dd| crate::gta::drawable::find_texture(&r, dd, g.tex, crate::gta::loader::MAX_TEX))
            .or_else(|| dds.get(&g.tex).and_then(|p| std::fs::read(p).ok()).and_then(|b| read_dds(&b, crate::gta::loader::MAX_TEX)));
        match found {
            Some(t) => {
                textures.push((key, t));
                part_keys.push(key);
            }
            None => {
                missing.push(format!("{:08x}", g.tex));
                part_keys.push(0);
            }
        }
    }
    let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for v in &model.verts {
        let p = v3(v.pos[0], v.pos[1], v.pos[2]);
        mn = mn.min(p);
        mx = mx.max(p);
    }
    let tris = model.idx.len() / 3;
    Ok(CustomModel { name, file: path.to_path_buf(), model, textures, part_keys, bb_min: mn, bb_max: mx, tris, missing_textures: missing })
}
