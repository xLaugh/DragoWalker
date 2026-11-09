//! Lecture des modèles 3D du jeu (ydr, ydd, yft) pour l'aperçu des props.
//! Ordre des champs : DrawableBase::Read (0x06004b91), DrawableModel::Read (0x06004a14),
//! DrawableGeometry::Read (0x06004a3f), VertexBuffer::Read (0x06004a64), IndexBuffer::Read (0x06004ad6),
//! DrawableDictionary::Read (0x06004bf0), FragType::Read (0x06004dc6).

use super::rpf::Resource;

#[derive(Default, Clone)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Mesh {
    fn finish(mut self) -> Option<Mesh> {
        if self.tris.is_empty() {
            return None;
        }
        let mut mn = [f32::MAX; 3];
        let mut mx = [f32::MIN; 3];
        for p in &self.pos {
            for k in 0..3 {
                mn[k] = mn[k].min(p[k]);
                mx[k] = mx[k].max(p[k]);
            }
        }
        self.min = mn;
        self.max = mx;
        Some(self)
    }
}

/// Liste de pointeurs (ResourcePointerList64) : en-tête { pointeur, nombre, capacité }.
fn pointer_list(r: &Resource, header: usize) -> Vec<usize> {
    let Some(arr) = r.ptr(r.u64(header)) else { return Vec::new() };
    let n = r.u16(header + 8) as usize;
    (0..n.min(4096)).filter_map(|i| r.ptr(r.u64(arr + i * 8))).collect()
}

fn read_geometry(r: &Resource, g: usize, mesh: &mut Mesh, max_tris: usize) {
    let Some(vb) = r.ptr(r.u64(g + 0x18)) else { return };
    let Some(ib) = r.ptr(r.u64(g + 0x38)) else { return };
    let stride = r.u16(vb + 0x08) as usize;
    let vcount = r.u32(vb + 0x18) as usize;
    let Some(vdata) = r.ptr(r.u64(vb + 0x10)).or_else(|| r.ptr(r.u64(vb + 0x20))) else { return };
    let icount = r.u32(ib + 0x08) as usize;
    let Some(idata) = r.ptr(r.u64(ib + 0x10)) else { return };
    if stride < 12 || stride > 256 || vcount == 0 || vcount > 200_000 || vdata + stride * vcount > r.data.len() || idata + icount * 2 > r.data.len() {
        return;
    }
    let base = mesh.pos.len() as u32;
    for i in 0..vcount {
        mesh.pos.push(r.v3(vdata + i * stride));
    }
    for t in 0..icount / 3 {
        if mesh.tris.len() >= max_tris {
            break;
        }
        let a = r.u16(idata + t * 6) as u32;
        let b = r.u16(idata + t * 6 + 2) as u32;
        let c = r.u16(idata + t * 6 + 4) as u32;
        if (a as usize) < vcount && (b as usize) < vcount && (c as usize) < vcount {
            mesh.tris.push([base + a, base + b, base + c]);
        }
    }
}

/// Modèle (DrawableBase) à la position `d` : niveau de détail le plus haut qui tient dans `max_tris`.
pub fn read_drawable(r: &Resource, d: usize, max_tris: usize) -> Option<Mesh> {
    // pointeurs High, Medium, Low, VeryLow à 0x50..0x68
    for lod in 0..4 {
        let Some(h) = r.ptr(r.u64(d + 0x50 + lod * 8)) else { continue };
        let mut mesh = Mesh::default();
        for m in pointer_list(r, h) {
            let Some(garr) = r.ptr(r.u64(m + 0x08)) else { continue };
            let gn = r.u16(m + 0x10) as usize;
            for gi in 0..gn.min(512) {
                if let Some(g) = r.ptr(r.u64(garr + gi * 8)) {
                    read_geometry(r, g, &mut mesh, max_tris);
                }
            }
        }
        if mesh.tris.len() < max_tris || lod == 3 {
            if let Some(m) = mesh.finish() {
                return Some(m);
            }
        }
    }
    None
}

/// ydr : le modèle commence au début de la ressource.
pub fn read_ydr(r: &Resource, max_tris: usize) -> Option<Mesh> {
    read_drawable(r, 0, max_tris)
}

/// ydd : dictionnaire de modèles, on prend celui dont le nom a le bon hachage.
pub fn read_ydd(r: &Resource, name_hash: u32, max_tris: usize) -> Option<Mesh> {
    let hashes = r.ptr(r.u64(0x20))?;
    let n = r.u16(0x28) as usize;
    let draws = r.ptr(r.u64(0x30))?;
    let mut idx = None;
    for i in 0..n.min(4096) {
        if r.u32(hashes + i * 4) == name_hash {
            idx = Some(i);
        }
    }
    let i = idx.unwrap_or(0);
    let d = r.ptr(r.u64(draws + i * 8))?;
    read_drawable(r, d, max_tris)
}

/// yft : objet cassable, son modèle principal est pointé à 0x30.
pub fn read_yft(r: &Resource, max_tris: usize) -> Option<Mesh> {
    let d = r.ptr(r.u64(0x30))?;
    read_drawable(r, d, max_tris)
}

// ---------------------------------------------------------------- modèles complets pour l'affichage

/// Un sommet prêt pour la carte graphique (36 octets).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct RVert {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
    pub col: [u8; 4],
}

/// Une partie d'objet avec son matériau (texture de couleur).
#[derive(Clone)]
pub struct RGeom {
    pub first: u32,
    pub count: u32,
    /// hachage du nom de la texture de couleur (DiffuseSampler), 0 si aucune
    pub tex: u32,
    /// la transparence de la texture découpe la forme (feuillages, grilles : RenderBucket 3)
    pub cutout: bool,
    /// partie mélangée par-dessus ce qui est derrière : 0 non, 1 décalque (RenderBucket 2),
    /// 2 transparent / verre (RenderBucket 1) — ShaderManager::RenderQueued 0x060009d4
    pub blend: u8,
    /// matériau de verre (vitres) : shader glass*.sps ou vehicle_vehglass*.sps
    pub glass: bool,
}

#[derive(Clone)]
pub struct RModel {
    pub verts: Vec<RVert>,
    pub idx: Vec<u32>,
    pub geoms: Vec<RGeom>,
    /// position du dictionnaire de textures intégré au modèle
    pub embedded_dict: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TexFmt {
    Dxt1,
    Dxt3,
    Dxt5,
    Bc7,
    Rgba8,
}

#[derive(Clone)]
pub struct TexData {
    pub w: u32,
    pub h: u32,
    pub fmt: TexFmt,
    /// niveaux de détail (mip), du plus grand au plus petit
    pub levels: Vec<Vec<u8>>,
}

/// Taille d'un composant de sommet (VertexComponentTypes::GetSizeInBytes 0x06005a4b).
fn comp_size(t: u8) -> usize {
    match t {
        1 => 4,
        2 => 4,
        3 => 8,
        5 => 8,
        6 => 12,
        7 => 16,
        8 | 9 | 10 => 4,
        _ => 0,
    }
}

fn half(b: u16) -> f32 {
    let s = if b & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((b >> 10) & 0x1F) as i32;
    let m = (b & 0x3FF) as f32;
    if e == 0 {
        s * m * (1.0 / 16_777_216.0)
    } else if e == 31 {
        0.0
    } else {
        s * (1.0 + m / 1024.0) * 2f32.powi(e - 15)
    }
}

/// Déclaration des sommets : position et type de chaque composant (bit i des drapeaux = composant i).
fn vertex_decl(r: &Resource, vb: usize) -> Option<(usize, [Option<(usize, u8)>; 16])> {
    let info = r.ptr(r.u64(vb + 0x30))?;
    let flags = r.u32(info);
    let stride = r.u16(info + 4) as usize;
    let types = r.u64(info + 8);
    let mut out = [None; 16];
    let mut acc = 0;
    for (i, o) in out.iter_mut().enumerate() {
        if flags >> i & 1 == 1 {
            let t = ((types >> (i * 4)) & 0xF) as u8;
            *o = Some((acc, t));
            acc += comp_size(t);
        }
    }
    Some((stride.max(acc), out))
}

fn read_vec3(r: &Resource, p: usize, t: u8) -> [f32; 3] {
    match t {
        6 | 7 => r.v3(p),
        3 => [half(r.u16(p)), half(r.u16(p + 2)), half(r.u16(p + 4))],
        9 | 8 => [r.u8(p) as f32 / 127.5 - 1.0, r.u8(p + 1) as f32 / 127.5 - 1.0, r.u8(p + 2) as f32 / 127.5 - 1.0],
        10 => [r.u8(p) as i8 as f32 / 127.0, r.u8(p + 1) as i8 as f32 / 127.0, r.u8(p + 2) as i8 as f32 / 127.0],
        _ => [0.0, 0.0, 1.0],
    }
}

fn read_uv(r: &Resource, p: usize, t: u8) -> [f32; 2] {
    match t {
        5 => [r.f32(p), r.f32(p + 4)],
        1 => [half(r.u16(p)), half(r.u16(p + 2))],
        _ => [0.0, 0.0],
    }
}

/// Nom (hachage) de la texture de couleur de chaque matériau du groupe de shaders.
/// ShaderGroup::Read 0x0600481f, ShaderFX::Read 0x06004847, ShaderParametersBlock::Read 0x0600487d.
/// Shaders de verre : ceux que CodeWalker range dans GlassBatches (ShaderRenderBucket::GroupBatches 0x060009ee) —
/// glass, glass_pv, glass_pv_env, glass_env, glass_spec, glass_reflect, glass_emissive, glass_emissivenight,
/// glass_emissivenight_alpha, glass_emissive_alpha, glass_breakable, glass_breakable_screendooralpha,
/// glass_displacement, glass_normal_spec_reflect, vehicle_vehglass, vehicle_vehglass_inner (.sps).
pub const GLASS_SHADERS: [u32; 16] = [
    0xEA2C_1235, 0xEF89_4EB8, 0xA6EC_ED02, 0x4B48_C1E2, 0xCA97_E0B5, 0x5A9D_C11F, 0xE9E4_2E78, 0x31E3_A82E,
    0x1CF3_5907, 0x7A86_3378, 0x5104_FB9E, 0xFC8C_DEEA, 0x19A6_1264, 0xAADD_A4C8, 0xB88D_C892, 0x2315_A09A,
];

/// Texture de couleur et genre de chaque matériau : 0 opaque, 1 découpe, 2 décalque, 3 transparent, 4 verre.
fn shader_textures(r: &Resource, sg: usize) -> Vec<(u32, u8)> {
    let diffuse = super::crypto::joaat("diffusesampler");
    let mut out = Vec::new();
    let Some(arr) = r.ptr(r.u64(sg + 0x10)) else { return out };
    let n = r.u16(sg + 0x18) as usize;
    for i in 0..n.min(256) {
        let mut tex = 0u32;
        let mut cutout = 0u8;
        if let Some(fx) = r.ptr(r.u64(arr + i * 8)) {
            // RenderBucket (ShaderFX +0x11) : 0 opaque, 1 transparent, 2 décalque, 3 découpe
            cutout = match r.u8(fx + 0x11) {
                3 => 1,
                2 => 2,
                1 => 3,
                _ => 0,
            };
            // nom du fichier du shader (ShaderFX +0x18, ShaderFX::Read 0x06004847 IL_019f) : le verre est dessiné
            // avec les transparents, après l'eau (GlassBatches)
            if GLASS_SHADERS.contains(&r.u32(fx + 0x18)) {
                cutout = 4;
            }
            if let Some(params) = r.ptr(r.u64(fx)) {
                let count = r.u8(fx + 0x10) as usize;
                let psize = r.u16(fx + 0x14) as usize;
                let hashes = params + psize;
                let mut first_tex = 0u32;
                for k in 0..count.min(64) {
                    let p = params + k * 16;
                    if r.u8(p) != 0 {
                        continue;
                    }
                    let Some(t) = r.ptr(r.u64(p + 8)) else { continue };
                    let Some(np) = r.ptr(r.u64(t + 0x28)) else { continue };
                    let end = r.data[np..].iter().take(128).position(|&b| b == 0).unwrap_or(0);
                    let name = String::from_utf8_lossy(&r.data[np..np + end]).to_lowercase();
                    if name.is_empty() {
                        continue;
                    }
                    let h = super::crypto::joaat(&name);
                    // BasicShader::SetGeomVars (0x060009fa) : texture de couleur = DiffuseSampler, PlateBgSampler
                    // ou DistanceMapSampler ; à défaut FlowSampler / FoamSampler / 0x991CB1BE — jamais « la première
                    // texture venue » (elle prenait des cartes de relief violettes, ex. le verre de l'abri du métro)
                    let ph = r.u32(hashes + k * 4);
                    if ph == diffuse || ph == 0x5002_2388 || ph == 0x605F_CC60 {
                        tex = h;
                        break;
                    }
                    if first_tex == 0 && matches!(ph, 0x485F_22B0 | 0xC2B0_8918 | 0x991E_CEBE) {
                        first_tex = h;
                    }
                }
                if tex == 0 {
                    tex = first_tex;
                }
            }
        }
        out.push((tex, cutout));
    }
    out
}

fn read_render_geometry(r: &Resource, g: usize, model: &mut RModel, tex: u32, kind: u8, max_tris: usize) {
    let glass = kind == 4;
    let (cutout, blend) = (kind == 1, match kind {
        2 => 1u8,
        3 | 4 => 2u8,
        _ => 0u8,
    });
    let Some(vb) = r.ptr(r.u64(g + 0x18)) else { return };
    let Some(ib) = r.ptr(r.u64(g + 0x38)) else { return };
    let Some((stride, decl)) = vertex_decl(r, vb) else { return };
    let vcount = r.u32(vb + 0x18) as usize;
    let Some(vdata) = r.ptr(r.u64(vb + 0x10)).or_else(|| r.ptr(r.u64(vb + 0x20))) else { return };
    let icount = r.u32(ib + 0x08) as usize;
    let Some(idata) = r.ptr(r.u64(ib + 0x10)) else { return };
    if stride < 12 || stride > 256 || vcount == 0 || vcount > 300_000 || vdata + stride * vcount > r.data.len() || idata + icount * 2 > r.data.len() {
        return;
    }
    let Some((pos_off, pos_t)) = decl[0] else { return };
    let base = model.verts.len() as u32;
    let has_nrm = decl[3];
    for i in 0..vcount {
        let v = vdata + i * stride;
        let mut rv = RVert { pos: read_vec3(r, v + pos_off, pos_t), nrm: [0.0, 0.0, 1.0], uv: [0.0, 0.0], col: [255, 255, 255, 255] };
        if let Some((o, t)) = has_nrm {
            rv.nrm = read_vec3(r, v + o, t);
        }
        if let Some((o, t)) = decl[6] {
            rv.uv = read_uv(r, v + o, t);
        }
        if let Some((o, 9)) = decl[4] {
            // couleur de sommet (B, G, R, A en mémoire)
            rv.col = [r.u8(v + o + 2), r.u8(v + o + 1), r.u8(v + o), r.u8(v + o + 3)];
        }
        model.verts.push(rv);
    }
    let first = model.idx.len() as u32;
    for t in 0..icount / 3 {
        if model.idx.len() / 3 >= max_tris {
            break;
        }
        let a = r.u16(idata + t * 6) as u32;
        let b = r.u16(idata + t * 6 + 2) as u32;
        let c = r.u16(idata + t * 6 + 4) as u32;
        if (a as usize) < vcount && (b as usize) < vcount && (c as usize) < vcount {
            model.idx.extend_from_slice(&[base + a, base + b, base + c]);
        }
    }
    // sans normales dans le fichier : normales calculées
    if has_nrm.is_none() {
        let n = model.verts.len();
        let mut acc = vec![[0f32; 3]; n - base as usize];
        for t in model.idx[first as usize..].chunks(3) {
            let p = |i: u32| model.verts[i as usize].pos;
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            for &i in t {
                let k = (i - base) as usize;
                for j in 0..3 {
                    acc[k][j] += cr[j];
                }
            }
        }
        for (k, a) in acc.iter().enumerate() {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
            model.verts[base as usize + k].nrm = [a[0] / l, a[1] / l, a[2] / l];
        }
    }
    let count = model.idx.len() as u32 - first;
    if count > 0 {
        model.geoms.push(RGeom { first, count, tex, cutout, blend, glass });
    }
}

/// Modèle complet (formes, normales, coordonnées de texture, matériaux) au niveau de détail le plus haut.
pub fn read_render_drawable(r: &Resource, d: usize, max_tris: usize) -> Option<RModel> {
    let sg = r.ptr(r.u64(d + 0x10));
    let shader_tex = sg.map(|s| shader_textures(r, s)).unwrap_or_default();
    let embedded_dict = sg.and_then(|s| r.ptr(r.u64(s + 0x08)));
    for lod in 0..4 {
        let Some(h) = r.ptr(r.u64(d + 0x50 + lod * 8)) else { continue };
        let mut model = RModel { verts: Vec::new(), idx: Vec::new(), geoms: Vec::new(), embedded_dict };
        for m in pointer_list(r, h) {
            let Some(garr) = r.ptr(r.u64(m + 0x08)) else { continue };
            let gn = r.u16(m + 0x10) as usize;
            let map = r.ptr(r.u64(m + 0x20));
            for gi in 0..gn.min(512) {
                let sidx = map.map(|p| r.u16(p + gi * 2) as usize).unwrap_or(usize::MAX);
                let (tex, cutout) = shader_tex.get(sidx).copied().unwrap_or((0, 0));
                if let Some(g) = r.ptr(r.u64(garr + gi * 8)) {
                    read_render_geometry(r, g, &mut model, tex, cutout, max_tris);
                }
            }
        }
        if !model.idx.is_empty() {
            return Some(model);
        }
    }
    None
}

/// Position du modèle dans un fichier : ydr au début, yft à 0x30, ydd selon le hachage du nom.
pub fn drawable_pos(r: &Resource, ext: &str, name_hash: u32) -> Option<usize> {
    match ext {
        "ydr" => Some(0),
        "yft" => r.ptr(r.u64(0x30)),
        _ => {
            let hashes = r.ptr(r.u64(0x20))?;
            let n = r.u16(0x28) as usize;
            let draws = r.ptr(r.u64(0x30))?;
            let i = (0..n.min(4096)).find(|&i| r.u32(hashes + i * 4) == name_hash)?;
            r.ptr(r.u64(draws + i * 8))
        }
    }
}

/// Texture d'un dictionnaire (TextureDictionary::Read 0x0600597b, Texture::Read 0x06005a0e),
/// en gardant les niveaux de 256 pixels au plus.
pub fn find_texture(r: &Resource, dict: usize, name_hash: u32, max_size: u32) -> Option<TexData> {
    let hashes = r.ptr(r.u64(dict + 0x20))?;
    let n = r.u16(dict + 0x28) as usize;
    let texs = r.ptr(r.u64(dict + 0x30))?;
    let i = (0..n.min(4096)).find(|&i| r.u32(hashes + i * 4) == name_hash)?;
    let t = r.ptr(r.u64(texs + i * 8))?;
    let w = r.u16(t + 0x50) as u32;
    let h = r.u16(t + 0x52) as u32;
    let stride = r.u16(t + 0x56) as u32;
    let fmt = match r.u32(t + 0x58) {
        827611204 => TexFmt::Dxt1, // « DXT1 »
        0x3354_5844 => TexFmt::Dxt3,
        0x3554_5844 => TexFmt::Dxt5,
        0x2037_4342 => TexFmt::Bc7,
        21 => TexFmt::Rgba8,
        _ => return None,
    };
    let levels = (r.u8(t + 0x5D) as u32).max(1);
    let mut p = r.ptr(r.u64(t + 0x70))?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let size_of = |lw: u32, lh: u32| -> usize {
        match fmt {
            TexFmt::Dxt1 => (lw.div_ceil(4).max(1) * lh.div_ceil(4).max(1) * 8) as usize,
            TexFmt::Dxt3 | TexFmt::Dxt5 | TexFmt::Bc7 => (lw.div_ceil(4).max(1) * lh.div_ceil(4).max(1) * 16) as usize,
            TexFmt::Rgba8 => (lw * lh * 4) as usize,
        }
    };
    let _ = stride;
    let mut out = TexData { w: 0, h: 0, fmt, levels: Vec::new() };
    let (mut lw, mut lh) = (w, h);
    for _ in 0..levels {
        let sz = size_of(lw, lh);
        if p + sz > r.data.len() {
            break;
        }
        let keep = lw.max(lh) <= max_size || out.levels.is_empty() && lw.max(lh) <= max_size;
        if keep {
            if out.levels.is_empty() {
                out.w = lw;
                out.h = lh;
            }
            let mut d = r.data[p..p + sz].to_vec();
            if fmt == TexFmt::Rgba8 {
                for px in d.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
            }
            out.levels.push(d);
        }
        p += sz;
        if lw == 1 && lh == 1 {
            break;
        }
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
        // les formats compressés s'arrêtent à 4×4
        if fmt != TexFmt::Rgba8 && (lw < 4 || lh < 4) {
            break;
        }
    }
    if out.levels.is_empty() {
        None
    } else {
        Some(out)
    }
}
