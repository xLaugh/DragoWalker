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
        let bones = bone_transforms(r, d);
        for m in pointer_list(r, h) {
            let Some(garr) = r.ptr(r.u64(m + 0x08)) else { continue };
            let gn = r.u16(m + 0x10) as usize;
            let v0 = mesh.pos.len();
            for gi in 0..gn.min(512) {
                if let Some(g) = r.ptr(r.u64(garr + gi * 8)) {
                    read_geometry(r, g, &mut mesh, max_tris);
                }
            }
            if let Some((q, t)) = model_bone(r, m, &bones) {
                for p in &mut mesh.pos[v0..] {
                    let q2 = quat_rotate(q, *p);
                    *p = [q2[0] + t[0], q2[1] + t[1], q2[2] + t[2]];
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
    /// couleur du sommet ; pour un sol à 4 couches : (Colour1.b, Colour1.g, Colour0.a, 255), les poids du mélange
    pub col: [u8; 4],
    /// deuxième coordonnée de texture (TexCoord1) : carte de mélange des sols à 4 couches (lookupSampler)
    pub uv1: [f32; 2],
}

/// Une partie d'objet avec son matériau (texture de couleur).
#[derive(Clone, Default)]
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
    /// sol à 4 couches (terrain_cb_w_4lyr*.sps) : textures TextureSampler_layer0..3 (hachages des noms)
    pub layers: [u32; 4],
    /// carte de mélange (lookupSampler), 0 si aucune
    pub mask: u32,
    /// 0 : pas un sol à couches ; 1 : poids = carte de mélange et couleur de sommet (selon Colour0.a) ;
    /// 2 : version lointaine (_lod) : couleur de sommet seule ; 3 : variantes _cm : carte de mélange seule
    pub terrain: u8,
}

/// Matériau d'une partie : texture de couleur, genre (0 opaque…), et pour un sol à 4 couches ses textures.
#[derive(Clone, Copy, Default)]
pub struct ShaderTex {
    pub tex: u32,
    pub kind: u8,
    pub layers: [u32; 4],
    pub mask: u32,
    pub terrain: u8,
}

/// Mode de mélange d'un sol à 4 couches d'après le fichier du shader (TerrainPS.hlsl de CodeWalker :
/// terrain_cb_w_4lyr_lod → couleur de sommet seule ; variantes _cm → carte de mélange seule ; sinon les deux).
fn terrain_mode(file_hash: u32) -> u8 {
    let j = super::crypto::joaat;
    if ["terrain_cb_w_4lyr_lod.sps", "terrain_cb_4lyr_lod.sps"].iter().any(|n| j(n) == file_hash) {
        return 2;
    }
    let cm = ["terrain_cb_w_4lyr_cm.sps", "terrain_cb_w_4lyr_cm_tnt.sps", "terrain_cb_w_4lyr_cm_pxm.sps", "terrain_cb_w_4lyr_cm_pxm_tnt.sps", "terrain_cb_4lyr_cm.sps", "terrain_cb_4lyr_cm_tnt.sps", "terrain_cb_4lyr_cm_pxm.sps", "terrain_cb_4lyr_cm_pxm_tnt.sps"];
    if cm.iter().any(|n| j(n) == file_hash) {
        return 3;
    }
    1
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
fn shader_textures(r: &Resource, sg: usize) -> Vec<ShaderTex> {
    let diffuse = super::crypto::joaat("diffusesampler");
    let layer_h = [super::crypto::joaat("texturesampler_layer0"), super::crypto::joaat("texturesampler_layer1"), super::crypto::joaat("texturesampler_layer2"), super::crypto::joaat("texturesampler_layer3")];
    let lookup_h = super::crypto::joaat("lookupsampler");
    let mut out = Vec::new();
    let Some(arr) = r.ptr(r.u64(sg + 0x10)) else { return out };
    let n = r.u16(sg + 0x18) as usize;
    for i in 0..n.min(256) {
        let mut tex = 0u32;
        let mut cutout = 0u8;
        let (mut layers, mut mask, mut terrain) = ([0u32; 4], 0u32, 0u8);
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
                    if let Some(li) = layer_h.iter().position(|x| *x == ph) {
                        layers[li] = h;
                        continue;
                    }
                    if ph == lookup_h {
                        mask = h;
                        continue;
                    }
                    if tex == 0 && (ph == diffuse || ph == 0x5002_2388 || ph == 0x605F_CC60) {
                        tex = h;
                    }
                    if first_tex == 0 && matches!(ph, 0x485F_22B0 | 0xC2B0_8918 | 0x991E_CEBE) {
                        first_tex = h;
                    }
                }
                if tex == 0 {
                    tex = first_tex;
                }
                if layers.iter().any(|x| *x != 0) {
                    terrain = terrain_mode(r.u32(fx + 0x18));
                }
                // sonde (DW_SHADER_DEBUG) : matériau sans texture de couleur → ses paramètres
                if tex == 0 && std::env::var_os("DW_SHADER_DEBUG").is_some() {
                    let names = ["diffusesampler", "texturesampler_layer0", "texturesampler_layer1", "texturesampler_layer2", "texturesampler_layer3", "bumpsampler_layer0", "bumpsampler_layer1", "bumpsampler_layer2", "bumpsampler_layer3", "lookupsampler", "bumpsampler", "specsampler", "heightmapsamplerlayer0", "heightmapsamplerlayer1", "heightmapsamplerlayer2", "heightmapsamplerlayer3", "detailsampler", "diffusesampler2", "texturegrasssampler", "distancemapsampler", "tintpalettesampler", "snowsampler", "dirtsampler", "environmentsampler", "diffuseextrasampler", "noisesampler", "diffusehfsampler", "texturesamp", "heightsampler", "flowsampler", "foamsampler"];
                    let sh = ["terrain_cb_w_4lyr.sps", "terrain_cb_w_4lyr_lod.sps", "terrain_cb_w_4lyr_spec.sps", "terrain_cb_w_4lyr_2tex.sps", "terrain_cb_w_4lyr_2tex_blend.sps", "terrain_cb_w_4lyr_2tex_blend_lod.sps", "terrain_cb_w_4lyr_cm.sps", "terrain_cb_w_4lyr_cm_tnt.sps", "terrain_cb_w_4lyr_cm_pxm.sps", "terrain_cb_w_4lyr_cm_pxm_tnt.sps", "terrain_cb_w_4lyr_pxm.sps", "terrain_cb_w_4lyr_pxm_spm.sps", "terrain_cb_w_4lyr_spec_pxm.sps", "terrain_cb_w_4lyr_spec_int.sps", "terrain_cb_w_4lyr_spec_int_pxm.sps", "terrain_cb_w_4lyr_2tex_pxm.sps", "terrain_cb_4lyr.sps", "terrain_cb_4lyr_lod.sps", "terrain_cb_4lyr_2tex.sps", "terrain_cb_4lyr_2tex_blend.sps", "terrain_cb_4lyr_2tex_blend_lod.sps", "terrain_cb_4lyr_cm.sps", "terrain_cb_4lyr_cm_tnt.sps", "terrain_cb_4lyr_pxm.sps", "terrain_cb_4lyr_spec.sps", "grass_batch.sps", "grass.sps", "default.sps", "normal.sps", "normal_spec.sps", "decal.sps", "water_terrainfoam.sps", "water_shallow.sps"];
                    let file = r.u32(fx + 0x18);
                    let fname = sh.iter().find(|n| super::crypto::joaat(n) == file).map(|s| s.to_string()).unwrap_or(format!("{file:08x}"));
                    let mut ps = Vec::new();
                    for k in 0..count.min(64) {
                        let p = params + k * 16;
                        let ph = r.u32(hashes + k * 4);
                        let pname = names.iter().find(|n| super::crypto::joaat(n) == ph).map(|s| s.to_string()).unwrap_or(format!("{ph:08x}"));
                        let tn = if r.u8(p) == 0 {
                            r.ptr(r.u64(p + 8)).and_then(|t| r.ptr(r.u64(t + 0x28))).map(|np| {
                                let end = r.data[np..].iter().take(128).position(|&b| b == 0).unwrap_or(0);
                                String::from_utf8_lossy(&r.data[np..np + end]).to_string()
                            }).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        if r.u8(p) == 0 {
                            ps.push(format!("{pname}={tn}"));
                        }
                    }
                    println!("   sans texture de couleur : shader {fname} ; textures : {}", ps.join(", "));
                }
            }
        }
        out.push(ShaderTex { tex, kind: cutout, layers, mask, terrain });
    }
    out
}

fn read_render_geometry(r: &Resource, g: usize, model: &mut RModel, st: ShaderTex, max_tris: usize) {
    let (tex, kind) = (st.tex, st.kind);
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
        let mut rv = RVert { pos: read_vec3(r, v + pos_off, pos_t), nrm: [0.0, 0.0, 1.0], uv: [0.0, 0.0], col: [255, 255, 255, 255], uv1: [0.0, 0.0] };
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
        if st.terrain != 0 {
            // sol à 4 couches : poids du mélange = Colour1.b et Colour1.g, et Colour0.a dose la carte de mélange
            // contre la couleur de sommet (TerrainPS.hlsl de CodeWalker)
            let a0 = match decl[4] {
                Some((o, 9)) => r.u8(v + o + 3),
                _ => 255,
            };
            let (b1, g1) = match decl[5] {
                Some((o, 9)) => (r.u8(v + o), r.u8(v + o + 1)),
                _ => (0, 0),
            };
            rv.col = [b1, g1, a0, 255];
            if let Some((o, t)) = decl[7] {
                rv.uv1 = read_uv(r, v + o, t);
            }
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
        model.geoms.push(RGeom { first, count, tex, cutout, blend, glass, layers: st.layers, mask: st.mask, terrain: st.terrain });
    }
}

/// Position de chaque os du squelette d'un modèle, dans le repère du modèle : (rotation x, y, z, w ; translation).
/// Squelette pointé à 0x18 du modèle (Skeleton : os à 0x20, nombre d'os à 0x5E) ; os de 0x50 octets : rotation
/// locale (quaternion) à 0x00, translation locale à 0x10, parent (i16) à 0x32 — d'après la structure Skeleton / Bone
/// de CodeWalker. Chaque os est composé avec son parent.
pub fn bone_transforms(r: &Resource, d: usize) -> Vec<([f32; 4], [f32; 3])> {
    let Some(sk) = r.ptr(r.u64(d + 0x18)) else { return Vec::new() };
    let Some(bones) = r.ptr(r.u64(sk + 0x20)) else { return Vec::new() };
    let n = r.u16(sk + 0x5E) as usize;
    if n == 0 || n > 512 || bones + n * 0x50 > r.data.len() {
        return Vec::new();
    }
    let mut abs: Vec<([f32; 4], [f32; 3])> = Vec::with_capacity(n);
    for i in 0..n {
        let b = bones + i * 0x50;
        let q = [r.f32(b), r.f32(b + 4), r.f32(b + 8), r.f32(b + 12)];
        let t = r.v3(b + 0x10);
        let parent = r.u16(b + 0x32) as i16;
        let ql = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        let q = if ql > 1e-6 && ql.is_finite() { [q[0] / ql, q[1] / ql, q[2] / ql, q[3] / ql] } else { [0.0, 0.0, 0.0, 1.0] };
        let t = if t.iter().all(|v| v.is_finite()) { t } else { [0.0; 3] };
        if parent >= 0 && (parent as usize) < i {
            let (pq, pt) = abs[parent as usize];
            let rt = quat_rotate(pq, t);
            abs.push((quat_mul(pq, q), [pt[0] + rt[0], pt[1] + rt[1], pt[2] + rt[2]]));
        } else {
            abs.push((q, t));
        }
    }
    abs
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    // v + 2 w (u × v) + 2 u × (u × v)
    let u = [q[0], q[1], q[2]];
    let c = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let uv = c(u, v);
    let uuv = c(u, uv);
    [v[0] + 2.0 * (q[3] * uv[0] + uuv[0]), v[1] + 2.0 * (q[3] * uv[1] + uuv[1]), v[2] + 2.0 * (q[3] * uv[2] + uuv[2])]
}

/// Os auquel une partie de modèle (DrawableModel) est attachée : SkeletonBinding à 0x28, os = 4e octet, peau = 2e
/// octet (une partie à peau suit ses sommets, elle n'est pas déplacée d'un bloc). None si rien à déplacer.
fn model_bone(r: &Resource, m: usize, bones: &[([f32; 4], [f32; 3])]) -> Option<([f32; 4], [f32; 3])> {
    let bind = r.u32(m + 0x28);
    let (bone, skin) = (((bind >> 24) & 0xFF) as usize, (bind >> 8) & 0xFF);
    if skin != 0 || bone == 0 || bone >= bones.len() {
        return None;
    }
    let (q, t) = bones[bone];
    if q == [0.0, 0.0, 0.0, 1.0] && t == [0.0; 3] {
        return None;
    }
    Some((q, t))
}

/// Modèle complet (formes, normales, coordonnées de texture, matériaux) au niveau de détail le plus haut.
pub fn read_render_drawable(r: &Resource, d: usize, max_tris: usize) -> Option<RModel> {
    let bones = bone_transforms(r, d);
    let debug_bones = std::env::var("DW_DEBUG_BONES").is_ok();
    if debug_bones {
        println!("squelette : {} os {:?}", bones.len(), bones.iter().map(|b| b.1).collect::<Vec<_>>());
    }
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
            let v0 = model.verts.len();
            for gi in 0..gn.min(512) {
                let sidx = map.map(|p| r.u16(p + gi * 2) as usize).unwrap_or(usize::MAX);
                let st = shader_tex.get(sidx).copied().unwrap_or_default();
                if let Some(g) = r.ptr(r.u64(garr + gi * 8)) {
                    read_render_geometry(r, g, &mut model, st, max_tris);
                }
            }
            // partie attachée à un os (tête d'un lampadaire, bras d'une barrière…) : placée sur cet os
            if let Some((q, t)) = model_bone(r, m, &bones) {
                for v in &mut model.verts[v0..] {
                    let p = quat_rotate(q, v.pos);
                    v.pos = [p[0] + t[0], p[1] + t[1], p[2] + t[2]];
                    v.nrm = quat_rotate(q, v.nrm);
                }
            }
            if debug_bones {
                let bind = r.u32(m + 0x28);
                println!("   partie : os {} peau {} -> {:?}", (bind >> 24) & 0xFF, (bind >> 8) & 0xFF, model_bone(r, m, &bones).map(|b| b.1));
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
    // formats de texture du jeu, comme DDSIO::GetDXGIFormat (0x0600050e) de CodeWalker ; les formats non
    // compressés et BC4/BC5 sont convertis en RGBA à la lecture (DDSIO::GetPixels 0x0600050a)
    let code = r.u32(t + 0x58);
    let fmt = match code {
        827611204 => TexFmt::Dxt1, // « DXT1 »
        0x3354_5844 => TexFmt::Dxt3,
        0x3554_5844 => TexFmt::Dxt5,
        0x2037_4342 => TexFmt::Bc7,
        21 | 22 | 25 | 28 | 32 | 50 | 826889281 | 843666497 => TexFmt::Rgba8,
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
            TexFmt::Rgba8 => crate::texdecode::raw_size(code, lw as usize, lh as usize),
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
            let d = r.data[p..p + sz].to_vec();
            let d = if fmt == TexFmt::Rgba8 { crate::texdecode::raw_to_rgba(code, lw as usize, lh as usize, &d) } else { d };
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

/// Lumière rangée dans un modèle (LightAttributes, 168 octets : LightAttributes::get_BlockLength 0x06004add),
/// champs dans l'ordre de la classe de CodeWalker. Liste à +0xB0 du Drawable (Drawable::Read 0x06004bad : après
/// DrawableBase, 0xA8 octets, et le nom ; Drawable::get_BlockLength 0x06004b9e = 0xD0).
#[derive(Clone, Copy, Debug, Default)]
pub struct GameLight {
    #[allow(dead_code)]
    pub pos: [f32; 3],
    pub color: [u8; 3],
    pub flashiness: u8,
    pub intensity: f32,
    pub flags: u32,
    pub kind: u8,
    pub time_flags: u32,
    pub falloff: f32,
    pub falloff_exp: f32,
    pub corona_size: f32,
    pub corona_intensity: f32,
    pub volume_intensity: f32,
    pub shadow_near_clip: f32,
    pub light_fade: u8,
    pub dir: [f32; 3],
    pub cone_inner: f32,
    pub cone_outer: f32,
    pub extent: [f32; 3],
}

/// Lumières d'un modèle simple (ydr / ydd ; les yft rangent les leurs ailleurs).
pub fn read_lights(r: &Resource, d: usize) -> Vec<GameLight> {
    let Some(p) = r.ptr(r.u64(d + 0xB0)) else { return Vec::new() };
    let n = r.u16(d + 0xB8) as usize;
    (0..n.min(512))
        .map(|i| {
            let a = p + i * 168;
            GameLight {
                pos: r.v3(a + 0x08),
                color: [r.u8(a + 0x18), r.u8(a + 0x19), r.u8(a + 0x1A)],
                flashiness: r.u8(a + 0x1B),
                intensity: r.f32(a + 0x1C),
                flags: r.u32(a + 0x20),
                kind: r.u8(a + 0x26),
                time_flags: r.u32(a + 0x28),
                falloff: r.f32(a + 0x2C),
                falloff_exp: r.f32(a + 0x30),
                volume_intensity: r.f32(a + 0x4C),
                corona_size: r.f32(a + 0x5C),
                light_fade: r.u8(a + 0x64),
                shadow_near_clip: r.f32(a + 0x68),
                corona_intensity: r.f32(a + 0x6C),
                dir: r.v3(a + 0x74),
                cone_inner: r.f32(a + 0x8C),
                cone_outer: r.f32(a + 0x90),
                extent: r.v3(a + 0x94),
            }
        })
        .collect()
}
