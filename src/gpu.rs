//! Affichage des vrais modèles 3D du jeu avec la carte graphique (OpenGL, via egui_glow).
//! Les modèles et leurs textures arrivent des fils de chargement (gta::loader) et sont envoyés
//! à la carte graphique petit à petit ; ceux qui ne sont pas encore prêts sont dessinés en boîte.

use crate::gta::drawable::TexFmt;
use crate::gta::loader::{self, Loaded, ModelSource};
use eframe::egui;
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Un objet à dessiner.
#[derive(Clone, Copy)]
pub struct Inst {
    pub arch: u32,
    /// matrice monde (colonnes), rotation × échelle puis position
    pub m: [f32; 16],
    pub color: [f32; 3],
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
}

pub struct FrameDraw {
    pub vp: [f32; 16],
    pub eye: [f32; 3],
    pub insts: Vec<Inst>,
    pub fog: [f32; 4],
    pub sea: bool,
    /// pixel (en points d'écran) où lire la profondeur : ce que la souris touche
    pub probe: Option<[f32; 2]>,
    /// ombres du soleil : matrice de la vue du soleil (None = pas d'ombres)
    pub light_vp: Option<[f32; 16]>,
    /// zones d'eau du jeu (water.xml) : min x, min y, max x, max y, hauteur
    pub water: Arc<Vec<[f32; 5]>>,
    /// temps (secondes) pour animer l'eau
    pub time: f32,
    /// tout ce qui dépasse cette hauteur n'est pas dessiné (coupe pour voir dans les bâtiments)
    pub clip_z: f32,
}

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    /// (premier indice, nombre, texture, découpe, mélangé, sol à couches, textures des couches + carte de mélange)
    parts: Vec<Part>,
    bytes: usize,
    last: u64,
}

/// Une partie d'un modèle envoyé à la carte graphique.
type Part = (u32, u32, u64, bool, u8, u8, [u64; 5]);

enum Slot {
    Loading,
    Ready(GpuMesh),
    Missing,
}

struct GpuTex {
    tex: glow::Texture,
    bytes: usize,
    last: u64,
}

struct Prog {
    p: glow::Program,
    u_vp: Option<glow::UniformLocation>,
    u_m: Option<glow::UniformLocation>,
    u_cube: Option<glow::UniformLocation>,
    u_bmin: Option<glow::UniformLocation>,
    u_bmax: Option<glow::UniformLocation>,
    u_tex: Option<glow::UniformLocation>,
    u_has_tex: Option<glow::UniformLocation>,
    u_color: Option<glow::UniformLocation>,
    u_eye: Option<glow::UniformLocation>,
    u_fog: Option<glow::UniformLocation>,
    u_alpha: Option<glow::UniformLocation>,
    u_cutout: Option<glow::UniformLocation>,
    u_blend: Option<glow::UniformLocation>,
    u_light_vp: Option<glow::UniformLocation>,
    u_shadow: Option<glow::UniformLocation>,
    u_shadow_on: Option<glow::UniformLocation>,
    u_water: Option<glow::UniformLocation>,
    u_time: Option<glow::UniformLocation>,
    u_clip: Option<glow::UniformLocation>,
    u_terrain: Option<glow::UniformLocation>,
    u_has_l: Option<glow::UniformLocation>,
    u_has_mask: Option<glow::UniformLocation>,
    u_layers: [Option<glow::UniformLocation>; 5],
}

pub struct Gpu {
    prog: Option<Prog>,
    pub error: Option<String>,
    slots: HashMap<u32, Slot>,
    textures: HashMap<u64, GpuTex>,
    rx: Receiver<Loaded>,
    tx: Sender<u32>,
    claimed: Arc<Mutex<HashSet<u64>>>,
    pending: Vec<Loaded>,
    pub in_flight: usize,
    cube: Option<GpuMesh>,
    frame: u64,
    mesh_bytes: usize,
    tex_bytes: usize,
    s3tc: bool,
    bptc: bool,
    pub stats: (usize, usize, usize),
    /// dernière profondeur lue : (position écran, profondeur 0..1)
    pub probe: Option<([f32; 2], f32)>,
    shadow_fbo: Option<glow::Framebuffer>,
    shadow_tex: Option<glow::Texture>,
    /// modèles fabriqués par DragoWalker (terrain, MLO importés, Blender) : copie gardée en mémoire, renvoyée à la
    /// carte graphique s'ils ont été libérés (on ne peut pas les relire dans les fichiers du jeu ; sans elle, ils
    /// devenaient des boîtes beiges) — comme CodeWalker, qui garde la source de chaque élément libéré et le recharge
    /// depuis elle (RenderableCacheLookup::RenderThreadSync 0x06000943 → RenderableCacheItem.Load)
    custom_src: HashMap<u32, Loaded>,
    /// leurs textures : jamais libérées (un modèle resté chargé ne saurait pas les redemander)
    pinned_tex: HashSet<u64>,
}

/// Côté de la carte des ombres (pixels) — choix de DragoWalker.
pub const SHADOW_SIZE: i32 = 2048;

/// Requêtes en cours au plus (les plus proches d'abord).
const MAX_IN_FLIGHT: usize = 256;
/// Mémoire graphique visée pour les modèles et les textures.
const MESH_BUDGET: usize = 1_400_000_000;
const TEX_BUDGET: usize = 700_000_000;

impl Gpu {
    /// Mémoire graphique prise par les modèles et les textures (Mo).
    pub fn memory_mb(&self) -> (usize, usize) {
        (self.mesh_bytes >> 20, self.tex_bytes >> 20)
    }

    pub fn new(src: ModelSource) -> Gpu {
        let claimed = Arc::new(Mutex::new(HashSet::new()));
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) - 1;
        let (tx, rx) = loader::spawn_workers(src, n.max(2), claimed.clone());
        Gpu {
            prog: None,
            error: None,
            slots: HashMap::new(),
            textures: HashMap::new(),
            rx,
            tx,
            claimed,
            pending: Vec::new(),
            in_flight: 0,
            cube: None,
            frame: 0,
            mesh_bytes: 0,
            tex_bytes: 0,
            s3tc: false,
            bptc: false,
            stats: (0, 0, 0),
            probe: None,
            shadow_fbo: None,
            shadow_tex: None,
            custom_src: HashMap::new(),
            pinned_tex: HashSet::new(),
        }
    }

    /// Demande le modèle d'un type d'objet s'il n'est pas déjà connu. Rend vrai s'il est prêt.
    pub fn want(&mut self, arch: u32) -> bool {
        match self.slots.get(&arch) {
            Some(Slot::Ready(_)) => true,
            Some(_) => false,
            None => {
                // modèle fabriqué par DragoWalker libéré : renvoyé depuis sa copie en mémoire
                if let Some(src) = self.custom_src.get(&arch) {
                    let l = Loaded { arch, model: src.model.clone(), part_keys: src.part_keys.clone(), layer_keys: src.layer_keys.clone(), textures: src.textures.clone() };
                    self.slots.insert(arch, Slot::Loading);
                    self.pending.push(l);
                    return false;
                }
                if self.in_flight < MAX_IN_FLIGHT && self.tx.send(arch).is_ok() {
                    self.slots.insert(arch, Slot::Loading);
                    self.in_flight += 1;
                }
                false
            }
        }
    }

    /// Modèle personnel (Blender / Sollumz) : remplace celui du jeu du même nom.
    pub fn add_custom(&mut self, l: Loaded) {
        self.pinned_tex.extend(l.textures.iter().map(|(k, _)| *k));
        self.custom_src.insert(l.arch, Loaded { arch: l.arch, model: l.model.clone(), part_keys: l.part_keys.clone(), layer_keys: l.layer_keys.clone(), textures: l.textures.clone() });
        self.slots.insert(l.arch, Slot::Loading);
        self.pending.push(l);
    }

    pub fn busy(&self) -> bool {
        self.in_flight > 0 || !self.pending.is_empty()
    }

    /// Le modèle est prêt ou en route (pas absent ni introuvable).
    pub fn has_or_loading(&self, arch: u32) -> bool {
        matches!(self.slots.get(&arch), Some(Slot::Ready(_)) | Some(Slot::Loading)) || self.pending.iter().any(|l| l.arch == arch)
    }

    #[allow(dead_code)]
    pub fn has_model(&self, arch: u32) -> bool {
        matches!(self.slots.get(&arch), Some(Slot::Ready(_)))
    }

    unsafe fn compile(&mut self, gl: &glow::Context) -> Result<(), String> {
        let ver = egui_glow::ShaderVersion::get(gl);
        let head = match ver {
            egui_glow::ShaderVersion::Gl140 => "#version 140\n",
            egui_glow::ShaderVersion::Es300 => "#version 300 es\nprecision highp float;\n",
            v => return Err(format!("OpenGL trop ancien pour les vrais modèles ({v:?})")),
        };
        let vs = format!(
            "{head}in vec3 aPos; in vec3 aNrm; in vec2 aUv; in vec4 aCol; in vec2 aUv1;
uniform mat4 uVP; uniform mat4 uM; uniform float uCube; uniform vec3 uBMin; uniform vec3 uBMax;
out vec3 vN; out vec2 vUv; out vec3 vW; out vec4 vCol; out vec2 vUv1;
void main() {{
  vec3 p = uCube > 0.5 ? mix(uBMin, uBMax, aPos) : aPos;
  vec4 w = uM * vec4(p, 1.0);
  vW = w.xyz; vN = mat3(uM) * aNrm; vUv = aUv; vCol = aCol; vUv1 = aUv1;
  gl_Position = uVP * w;
}}"
        );
        let fs = format!(
            "{head}in vec3 vN; in vec2 vUv; in vec3 vW; in vec4 vCol; in vec2 vUv1;
uniform sampler2D uL0; uniform sampler2D uL1; uniform sampler2D uL2; uniform sampler2D uL3; uniform sampler2D uMask;
uniform vec4 uHasL; uniform float uHasMask; uniform float uTerrain;
uniform sampler2D uTex; uniform float uHasTex; uniform float uCutout; uniform float uBlend; uniform vec3 uColor; uniform vec3 uEye; uniform vec4 uFog; uniform float uAlpha;
uniform mat4 uLightVP; uniform sampler2D uShadow; uniform float uShadowOn; uniform float uWater; uniform float uTime; uniform float uClipZ;
out vec4 oCol;
float shadowAt(vec3 w, vec3 n, vec3 sun) {{
  vec4 lp = uLightVP * vec4(w + n * 0.08, 1.0);
  vec3 q = lp.xyz / lp.w * 0.5 + 0.5;
  if (q.x <= 0.0 || q.x >= 1.0 || q.y <= 0.0 || q.y >= 1.0 || q.z >= 1.0) return 1.0;
  float bias = 0.0006 + 0.0025 * (1.0 - max(dot(n, sun), 0.0));
  float lit = 0.0;
  vec2 texel = vec2(1.0 / 2048.0);
  for (int i = -1; i <= 1; i++) {{
    for (int j = -1; j <= 1; j++) {{
      float d = texture(uShadow, q.xy + vec2(float(i), float(j)) * texel).r;
      lit += (q.z - bias <= d) ? 1.0 : 0.0;
    }}
  }}
  return lit / 9.0;
}}
void main() {{
  if (vW.z > uClipZ) discard;
  vec3 sun = normalize(vec3(-0.45, -0.3, 0.85));
  vec3 base = uColor;
  float ta = 1.0;
  if (uHasTex > 0.5) {{
    vec4 t = texture(uTex, vUv);
    if (uCutout > 0.5 && t.a < 0.3) discard;
    if (uBlend > 0.5 && t.a < 0.02) discard;
    base = t.rgb;
    ta = t.a;
  }}
  if (uTerrain > 0.5) {{
    // sol à 4 couches, comme TerrainPS.hlsl de CodeWalker : poids B, G venant de Colour1 (vCol.rg) et / ou de la
    // carte de mélange (canaux b, g) dosée par Colour0.a (vCol.b) ; couche absente = gris moyen
    vec4 g = vec4(0.5, 0.5, 0.5, 1.0);
    vec4 c1 = uHasL.x > 0.5 ? texture(uL0, vUv) : g;
    vec4 c2 = uHasL.y > 0.5 ? texture(uL1, vUv) : g;
    vec4 c3 = uHasL.z > 0.5 ? texture(uL2, vUv) : g;
    vec4 c4 = uHasL.w > 0.5 ? texture(uL3, vUv) : g;
    vec2 vc = vCol.rg;
    vec2 m = uHasMask > 0.5 ? texture(uMask, vUv1).bg : vc;
    vec2 wt = uTerrain < 1.5 ? mix(m, vc, vCol.b) : (uTerrain < 2.5 ? vc : m);
    base = mix(mix(c1, c2, wt.x), mix(c3, c4, wt.x), wt.y).rgb;
  }}
  float d = length(vW - uEye);
  float f = clamp(1.0 - exp(-d * uFog.w), 0.0, 0.8);
  if (uWater > 0.5) {{
    // eau animée : petites vagues, reflet du ciel et éclat du soleil
    vec2 p = vW.xy;
    float t = uTime;
    vec3 wn = normalize(vec3(
      0.06 * sin(p.x * 0.21 + t * 1.3) + 0.04 * sin(p.x * 0.53 - p.y * 0.31 + t * 2.1) + 0.02 * sin((p.x + p.y) * 1.7 + t * 3.0),
      0.06 * cos(p.y * 0.19 - t * 1.1) + 0.04 * cos(p.y * 0.47 + p.x * 0.27 - t * 1.7) + 0.02 * cos((p.x - p.y) * 1.9 - t * 2.6),
      1.0));
    vec3 v = normalize(uEye - vW);
    float fres = pow(1.0 - max(dot(wn, v), 0.0), 3.0);
    vec3 deep = vec3(0.05, 0.20, 0.30);
    vec3 sky = vec3(0.62, 0.74, 0.86);
    vec3 col = mix(deep, sky, clamp(0.15 + fres, 0.0, 1.0));
    float spec = pow(max(dot(reflect(-sun, wn), v), 0.0), 80.0);
    col += vec3(1.0, 0.95, 0.85) * spec * 1.2;
    oCol = vec4(mix(col, uFog.rgb, f), 0.82);
    return;
  }}
  vec3 n = normalize(vN);
  if (!gl_FrontFacing) n = -n;
  float sh = uShadowOn > 0.5 ? shadowAt(vW, n, sun) : 1.0;
  float l = 0.5 + 0.62 * max(dot(n, sun), 0.0) * sh - 0.08 * (1.0 - sh);
  vec3 c = base * l;
  oCol = vec4(mix(c, uFog.rgb, f), uBlend > 0.5 ? ta : uAlpha);
}}"
        );
        let p = gl.create_program()?;
        let mut shaders = Vec::new();
        for (ty, src) in [(glow::VERTEX_SHADER, &vs), (glow::FRAGMENT_SHADER, &fs)] {
            let s = gl.create_shader(ty)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(gl.get_shader_info_log(s));
            }
            gl.attach_shader(p, s);
            shaders.push(s);
        }
        for (i, n) in ["aPos", "aNrm", "aUv", "aCol", "aUv1"].iter().enumerate() {
            gl.bind_attrib_location(p, i as u32, n);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(gl.get_program_info_log(p));
        }
        for s in shaders {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        let u = |n: &str| gl.get_uniform_location(p, n);
        self.prog = Some(Prog {
            p,
            u_vp: u("uVP"),
            u_m: u("uM"),
            u_cube: u("uCube"),
            u_bmin: u("uBMin"),
            u_bmax: u("uBMax"),
            u_tex: u("uTex"),
            u_has_tex: u("uHasTex"),
            u_color: u("uColor"),
            u_eye: u("uEye"),
            u_fog: u("uFog"),
            u_alpha: u("uAlpha"),
            u_cutout: u("uCutout"),
            u_blend: u("uBlend"),
            u_light_vp: u("uLightVP"),
            u_shadow: u("uShadow"),
            u_shadow_on: u("uShadowOn"),
            u_water: u("uWater"),
            u_time: u("uTime"),
            u_clip: u("uClipZ"),
            u_terrain: u("uTerrain"),
            u_has_l: u("uHasL"),
            u_has_mask: u("uHasMask"),
            u_layers: [u("uL0"), u("uL1"), u("uL2"), u("uL3"), u("uMask")],
        });
        // carte des ombres : texture de profondeur + cadre de dessin
        let st = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(st));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, SHADOW_SIZE, SHADOW_SIZE, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
        for (k, v) in [(glow::TEXTURE_MIN_FILTER, glow::NEAREST), (glow::TEXTURE_MAG_FILTER, glow::NEAREST), (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE), (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE)] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
        }
        let fb = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(st), 0);
        gl.draw_buffer(glow::NONE);
        gl.read_buffer(glow::NONE);
        let complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if complete {
            self.shadow_fbo = Some(fb);
            self.shadow_tex = Some(st);
        }
        let ext = gl.supported_extensions();
        self.s3tc = ext.iter().any(|e| e.contains("texture_compression_s3tc") || e.contains("compressed_texture_s3tc"));
        self.bptc = ext.iter().any(|e| e.contains("texture_compression_bptc"));
        // boîte unité (de 0 à 1) pour les objets pas encore chargés
        let mut verts: Vec<loader_vert::V> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            ([-1.0, 0.0, 0.0], [[0., 0., 0.], [0., 1., 0.], [0., 1., 1.], [0., 0., 1.]]),
            ([1.0, 0.0, 0.0], [[1., 0., 0.], [1., 0., 1.], [1., 1., 1.], [1., 1., 0.]]),
            ([0.0, -1.0, 0.0], [[0., 0., 0.], [0., 0., 1.], [1., 0., 1.], [1., 0., 0.]]),
            ([0.0, 1.0, 0.0], [[0., 1., 0.], [1., 1., 0.], [1., 1., 1.], [0., 1., 1.]]),
            ([0.0, 0.0, -1.0], [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]]),
            ([0.0, 0.0, 1.0], [[0., 0., 1.], [0., 1., 1.], [1., 1., 1.], [1., 0., 1.]]),
        ];
        for (n, q) in faces {
            let b = verts.len() as u32;
            for p in q {
                verts.push(loader_vert::V { pos: p, nrm: n, uv: [0.0, 0.0], col: [255; 4], uv1: [0.0, 0.0] });
            }
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        self.cube = Some(upload_mesh(gl, &verts, &idx, vec![(0, idx.len() as u32, 0, false, 0, 0, [0; 5])])?);
        Ok(())
    }

    unsafe fn upload_tex(&mut self, gl: &glow::Context, key: u64, t: &crate::gta::drawable::TexData) {
        let internal = match t.fmt {
            TexFmt::Dxt1 if self.s3tc => glow::COMPRESSED_RGBA_S3TC_DXT1_EXT,
            TexFmt::Dxt3 if self.s3tc => glow::COMPRESSED_RGBA_S3TC_DXT3_EXT,
            TexFmt::Dxt5 if self.s3tc => glow::COMPRESSED_RGBA_S3TC_DXT5_EXT,
            TexFmt::Bc7 if self.bptc => glow::COMPRESSED_RGBA_BPTC_UNORM,
            TexFmt::Rgba8 => glow::RGBA8,
            _ => return,
        };
        if let Some(old) = self.textures.remove(&key) {
            gl.delete_texture(old.tex);
            self.tex_bytes -= old.bytes;
        }
        let Ok(tex) = gl.create_texture() else { return };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        let (mut w, mut h) = (t.w as i32, t.h as i32);
        let mut bytes = 0;
        for (lvl, d) in t.levels.iter().enumerate() {
            if t.fmt == TexFmt::Rgba8 {
                gl.tex_image_2d(glow::TEXTURE_2D, lvl as i32, internal as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(d)));
            } else {
                gl.compressed_tex_image_2d(glow::TEXTURE_2D, lvl as i32, internal as i32, w, h, 0, d.len() as i32, d);
            }
            bytes += d.len();
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        let n = t.levels.len() as i32;
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, n - 1);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, if n > 1 { glow::LINEAR_MIPMAP_LINEAR } else { glow::LINEAR } as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
        self.tex_bytes += bytes;
        self.textures.insert(key, GpuTex { tex, bytes, last: self.frame });
    }

    /// Envoie à la carte graphique les modèles arrivés (au plus ~6 ms par image).
    unsafe fn upload_pending(&mut self, gl: &glow::Context) {
        while let Ok(l) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            self.pending.push(l);
        }
        let t0 = std::time::Instant::now();
        while let Some(l) = self.pending.pop() {
            for (k, t) in &l.textures {
                self.upload_tex(gl, *k, t);
            }
            match &l.model {
                Some(m) => {
                    let verts: Vec<loader_vert::V> = m.verts.iter().map(|v| loader_vert::V { pos: v.pos, nrm: v.nrm, uv: v.uv, col: v.col, uv1: v.uv1 }).collect();
                    let parts = m
                        .geoms
                        .iter()
                        .enumerate()
                        .map(|(i, g)| (g.first, g.count, l.part_keys.get(i).copied().unwrap_or(0), g.cutout, g.blend, g.terrain, l.layer_keys.get(i).copied().unwrap_or([0; 5])))
                        .collect();
                    match upload_mesh(gl, &verts, &m.idx, parts) {
                        Ok(mut gm) => {
                            gm.last = self.frame;
                            self.mesh_bytes += gm.bytes;
                            if let Some(Slot::Ready(old)) = self.slots.insert(l.arch, Slot::Ready(gm)) {
                                gl.delete_vertex_array(old.vao);
                                gl.delete_buffer(old.vbo);
                                gl.delete_buffer(old.ibo);
                                self.mesh_bytes -= old.bytes;
                            }
                        }
                        Err(_) => {
                            self.slots.insert(l.arch, Slot::Missing);
                        }
                    }
                }
                None => {
                    self.slots.insert(l.arch, Slot::Missing);
                }
            }
            if t0.elapsed().as_millis() > 6 {
                break;
            }
        }
    }

    /// Libère les modèles et textures les moins utilisés quand la mémoire visée est dépassée.
    unsafe fn evict(&mut self, gl: &glow::Context) {
        if self.mesh_bytes > MESH_BUDGET {
            let mut v: Vec<(u64, u32)> = self.slots.iter().filter_map(|(k, s)| if let Slot::Ready(m) = s { Some((m.last, *k)) } else { None }).collect();
            v.sort();
            for (last, k) in v {
                if self.mesh_bytes <= MESH_BUDGET * 8 / 10 || last + 30 > self.frame {
                    break;
                }
                if let Some(Slot::Ready(m)) = self.slots.remove(&k) {
                    gl.delete_vertex_array(m.vao);
                    gl.delete_buffer(m.vbo);
                    gl.delete_buffer(m.ibo);
                    self.mesh_bytes -= m.bytes;
                }
            }
        }
        if self.tex_bytes > TEX_BUDGET {
            let mut v: Vec<(u64, u64)> = self.textures.iter().filter(|(k, _)| !self.pinned_tex.contains(k)).map(|(k, t)| (t.last, *k)).collect();
            v.sort();
            for (last, k) in v {
                if self.tex_bytes <= TEX_BUDGET * 8 / 10 || last + 30 > self.frame {
                    break;
                }
                if let Some(t) = self.textures.remove(&k) {
                    gl.delete_texture(t.tex);
                    self.tex_bytes -= t.bytes;
                    if let Ok(mut c) = self.claimed.lock() {
                        c.remove(&k);
                    }
                }
            }
        }
    }

    /// Dessine les objets (vrais modèles, sinon boîtes). En passe d'ombres, les boîtes ne sont pas dessinées.
    /// `blend_pass` : 0 les parties pleines, 1 les décalques, 2 les transparences et le verre.
    unsafe fn draw_insts(&mut self, gl: &glow::Context, insts: &[Inst], shadow_pass: bool, blend_pass: u8) -> (usize, usize, usize) {
        let p = self.prog.as_ref().unwrap();
        let (u_m, u_cube, u_bmin, u_bmax, u_has, u_col, u_cut) = (p.u_m.clone(), p.u_cube.clone(), p.u_bmin.clone(), p.u_bmax.clone(), p.u_has_tex.clone(), p.u_color.clone(), p.u_cutout.clone());
        let (u_terr, u_has_l, u_has_mask) = (p.u_terrain.clone(), p.u_has_l.clone(), p.u_has_mask.clone());
        gl.uniform_1_f32(u_terr.as_ref(), 0.0);
        let frame = self.frame;
        let (mut n_mesh, mut n_box, mut n_tris) = (0, 0, 0);
        gl.active_texture(glow::TEXTURE0);
        for inst in insts {
            gl.uniform_matrix_4_f32_slice(u_m.as_ref(), false, &inst.m);
            gl.uniform_3_f32(u_col.as_ref(), inst.color[0], inst.color[1], inst.color[2]);
            match self.slots.get_mut(&inst.arch) {
                Some(Slot::Ready(m)) => {
                    m.last = frame;
                    gl.uniform_1_f32(u_cube.as_ref(), 0.0);
                    // vrai modèle : une partie sans texture de couleur est gris neutre (comme CodeWalker sans
                    // texture, EnableTexture = 0) — la couleur par type ne sert qu'aux boîtes
                    gl.uniform_3_f32(u_col.as_ref(), 0.72, 0.72, 0.72);
                    gl.bind_vertex_array(Some(m.vao));
                    for &(first, count, key, cutout, blend, terr, lk) in &m.parts {
                        // les décalques et transparences ne font pas d'ombre et sont dessinés après le reste
                        if blend != blend_pass {
                            continue;
                        }
                        // un décalque sans sa texture n'est pas dessiné (il cacherait la surface en dessous)
                        if blend != 0 && !self.textures.contains_key(&key) {
                            continue;
                        }
                        gl.uniform_1_f32(u_cut.as_ref(), if cutout { 1.0 } else { 0.0 });
                        let t = if key != 0 && (!shadow_pass || cutout) { self.textures.get_mut(&key) } else { None };
                        match t {
                            Some(t) => {
                                t.last = frame;
                                gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
                                gl.uniform_1_f32(u_has.as_ref(), 1.0);
                            }
                            None => gl.uniform_1_f32(u_has.as_ref(), 0.0),
                        }
                        // sol à 4 couches : textures des couches sur les unités 2 à 5, carte de mélange sur la 6
                        if terr != 0 && !shadow_pass {
                            let mut has = [0.0f32; 5];
                            for (i, k) in lk.iter().enumerate() {
                                if *k == 0 {
                                    continue;
                                }
                                if let Some(t) = self.textures.get_mut(k) {
                                    t.last = frame;
                                    gl.active_texture(glow::TEXTURE2 + i as u32);
                                    gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
                                    has[i] = 1.0;
                                }
                            }
                            gl.active_texture(glow::TEXTURE0);
                            gl.uniform_1_f32(u_terr.as_ref(), terr as f32);
                            gl.uniform_4_f32(u_has_l.as_ref(), has[0], has[1], has[2], has[3]);
                            gl.uniform_1_f32(u_has_mask.as_ref(), has[4]);
                        } else {
                            gl.uniform_1_f32(u_terr.as_ref(), 0.0);
                        }
                        gl.draw_elements(glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (first * 4) as i32);
                        n_tris += count as usize / 3;
                    }
                    n_mesh += 1;
                }
                _ => {
                    if shadow_pass || blend_pass != 0 {
                        continue;
                    }
                    if let Some(c) = &self.cube {
                        gl.uniform_1_f32(u_terr.as_ref(), 0.0);
                        gl.uniform_1_f32(u_cube.as_ref(), 1.0);
                        gl.uniform_3_f32(u_bmin.as_ref(), inst.bmin[0], inst.bmin[1], inst.bmin[2]);
                        gl.uniform_3_f32(u_bmax.as_ref(), inst.bmax[0], inst.bmax[1], inst.bmax[2]);
                        gl.uniform_1_f32(u_has.as_ref(), 0.0);
                        gl.bind_vertex_array(Some(c.vao));
                        gl.draw_elements(glow::TRIANGLES, 36, glow::UNSIGNED_INT, 0);
                        n_box += 1;
                    }
                }
            }
        }
        (n_mesh, n_box, n_tris)
    }

    /// Parties mélangées (1 décalques, 2 transparences) : alpha de la texture, profondeur non écrite, pas d'ombre.
    unsafe fn blend_pass(&mut self, gl: &glow::Context, fd: &FrameDraw, which: u8) {
        let (u_blend, u_shadow_on) = {
            let p = self.prog.as_ref().unwrap();
            (p.u_blend.clone(), p.u_shadow_on.clone())
        };
        gl.use_program(self.prog.as_ref().map(|p| p.p));
        let _ = u_shadow_on;
        gl.uniform_1_f32(u_blend.as_ref(), 1.0);
        gl.enable(glow::BLEND);
        gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
        gl.depth_mask(false);
        if which == 1 {
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(-1.0, -2.0);
        }
        self.draw_insts(gl, &fd.insts, false, which);
        gl.disable(glow::POLYGON_OFFSET_FILL);
        gl.depth_mask(true);
        gl.disable(glow::BLEND);
        gl.uniform_1_f32(u_blend.as_ref(), 0.0);
    }

    /// Dessin (appelé par egui pendant l'affichage de la vue).
    pub unsafe fn paint(&mut self, gl: &glow::Context, info: &egui::PaintCallbackInfo, fd: &FrameDraw) {
        self.frame += 1;
        if self.prog.is_none() && self.error.is_none() {
            if let Err(e) = self.compile(gl) {
                self.error = Some(e);
            }
        }
        let Some(prog) = self.prog.as_ref().map(|p| p.p) else { return };
        self.upload_pending(gl);
        self.evict(gl);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::BLEND);
        gl.use_program(Some(prog));
        let (u_vp, u_eye, u_fog, u_tex, u_alpha, u_light, u_shadow, u_shadow_on, u_water, u_time) = {
            let p = self.prog.as_ref().unwrap();
            (p.u_vp.clone(), p.u_eye.clone(), p.u_fog.clone(), p.u_tex.clone(), p.u_alpha.clone(), p.u_light_vp.clone(), p.u_shadow.clone(), p.u_shadow_on.clone(), p.u_water.clone(), p.u_time.clone())
        };
        gl.uniform_1_i32(u_tex.as_ref(), 0);
        gl.uniform_1_i32(u_shadow.as_ref(), 1);
        {
            let p = self.prog.as_ref().unwrap();
            for (i, u) in p.u_layers.iter().enumerate() {
                gl.uniform_1_i32(u.as_ref(), 2 + i as i32);
            }
            gl.uniform_1_f32(p.u_terrain.as_ref(), 0.0);
        }
        gl.uniform_1_f32(u_alpha.as_ref(), 1.0);
        gl.uniform_1_f32(u_water.as_ref(), 0.0);
        gl.uniform_1_f32(u_shadow_on.as_ref(), 0.0);
        gl.uniform_4_f32(u_fog.as_ref(), fd.fog[0], fd.fog[1], fd.fog[2], fd.fog[3]);
        gl.uniform_1_f32(u_time.as_ref(), fd.time);
        let u_clip = self.prog.as_ref().unwrap().u_clip.clone();
        gl.uniform_1_f32(u_clip.as_ref(), fd.clip_z);
        // 1. passe d'ombres : la scène vue du soleil, dans la carte de profondeur
        let shadows = match (fd.light_vp, self.shadow_fbo) {
            (Some(lvp), Some(fb)) => {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.disable(glow::SCISSOR_TEST);
                gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
                gl.color_mask(false, false, false, false);
                gl.clear_depth_f32(1.0);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.5, 3.0);
                gl.uniform_matrix_4_f32_slice(u_vp.as_ref(), false, &lvp);
                gl.uniform_3_f32(u_eye.as_ref(), fd.eye[0], fd.eye[1], fd.eye[2]);
                self.draw_insts(gl, &fd.insts, true, 0);
                gl.disable(glow::POLYGON_OFFSET_FILL);
                gl.color_mask(true, true, true, true);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                let vp = info.viewport_in_pixels();
                gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
                true
            }
            _ => false,
        };
        // 2. passe normale
        let clip = info.clip_rect_in_pixels();
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(clip.left_px, clip.from_bottom_px, clip.width_px, clip.height_px);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LEQUAL);
        gl.depth_mask(true);
        gl.clear_depth_f32(1.0);
        gl.clear(glow::DEPTH_BUFFER_BIT);
        gl.uniform_matrix_4_f32_slice(u_vp.as_ref(), false, &fd.vp);
        gl.uniform_3_f32(u_eye.as_ref(), fd.eye[0], fd.eye[1], fd.eye[2]);
        if shadows {
            gl.uniform_matrix_4_f32_slice(u_light.as_ref(), false, &fd.light_vp.unwrap());
            gl.uniform_1_f32(u_shadow_on.as_ref(), 1.0);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, self.shadow_tex);
            gl.active_texture(glow::TEXTURE0);
        }
        let (n_mesh, n_box, n_tris) = self.draw_insts(gl, &fd.insts, false, 0);
        // 2 bis. décalques (RenderBucket 2) juste après les parties pleines, mélangés, sans écrire la profondeur
        // (ShaderManager::RenderQueued 0x060009d4 : DecalBatches avec dsDisableWrite, IL_02cb–IL_0341),
        // légèrement avancés pour ne pas scintiller avec la surface qu'ils recouvrent
        self.blend_pass(gl, fd, 1);
        gl.uniform_1_f32(u_shadow_on.as_ref(), 0.0);
        // 3. l'eau : la mer (z = 0) et les zones d'eau du jeu (lacs, rivières, piscines), animées
        if fd.sea {
            if let Some(c) = &self.cube {
                let p = self.prog.as_ref().unwrap();
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                let id: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
                gl.uniform_matrix_4_f32_slice(p.u_m.as_ref(), false, &id);
                gl.uniform_1_f32(p.u_cube.as_ref(), 1.0);
                gl.uniform_1_f32(p.u_has_tex.as_ref(), 0.0);
                gl.uniform_1_f32(u_water.as_ref(), 1.0);
                gl.bind_vertex_array(Some(c.vao));
                // comme CodeWalker (ShaderManager::RenderQueued 0x060009d4, RenderWaterQuads) : seulement les zones
                // d'eau des fichiers water*.xml du jeu, mer comprise — pas de mer infinie à z = 0, qui apparaissait
                // sous les intérieurs souterrains (station de métro) et sous les trous du sol
                for q in fd.water.iter() {
                    // distance au point le plus proche de la zone
                    let cx = (q[0] - fd.eye[0]).max(0.0).max(fd.eye[0] - q[2]);
                    let cy = (q[1] - fd.eye[1]).max(0.0).max(fd.eye[1] - q[3]);
                    if cx * cx + cy * cy > 5000.0 * 5000.0 {
                        continue;
                    }
                    gl.uniform_3_f32(p.u_bmin.as_ref(), q[0], q[1], q[4] - 0.02);
                    gl.uniform_3_f32(p.u_bmax.as_ref(), q[2], q[3], q[4]);
                    gl.draw_elements(glow::TRIANGLES, 36, glow::UNSIGNED_INT, 0);
                }
                gl.uniform_1_f32(u_water.as_ref(), 0.0);
                gl.depth_mask(true);
            }
        }
        // 4. transparences et verre (RenderBucket 1) après l'eau, mélangés sans écrire la profondeur
        // (RenderQueued : AlphaBatches puis GlassBatches, IL_05d0–IL_060f)
        self.blend_pass(gl, fd, 2);
        self.stats = (n_mesh, n_box, n_tris);
        // profondeur sous la souris (après la mer, pour pouvoir viser l'eau)
        self.probe = None;
        if let Some(p) = fd.probe {
            let ppp = info.pixels_per_point;
            let x = (p[0] * ppp) as i32;
            let y = info.screen_size_px[1] as i32 - (p[1] * ppp) as i32 - 1;
            let mut buf = [0u8; 4];
            gl.read_pixels(x, y, 1, 1, glow::DEPTH_COMPONENT, glow::FLOAT, glow::PixelPackData::Slice(Some(&mut buf)));
            self.probe = Some((p, f32::from_le_bytes(buf)));
        }
        gl.bind_vertex_array(None);
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);
    }
}

/// Format des sommets envoyés à la carte graphique.
mod loader_vert {
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct V {
        pub pos: [f32; 3],
        pub nrm: [f32; 3],
        pub uv: [f32; 2],
        pub col: [u8; 4],
        pub uv1: [f32; 2],
    }
}

unsafe fn upload_mesh(gl: &glow::Context, verts: &[loader_vert::V], idx: &[u32], parts: Vec<Part>) -> Result<GpuMesh, String> {
    let vao = gl.create_vertex_array()?;
    gl.bind_vertex_array(Some(vao));
    let vbo = gl.create_buffer()?;
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    let vb: &[u8] = std::slice::from_raw_parts(verts.as_ptr() as *const u8, std::mem::size_of_val(verts));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vb, glow::STATIC_DRAW);
    let ibo = gl.create_buffer()?;
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
    let ib: &[u8] = std::slice::from_raw_parts(idx.as_ptr() as *const u8, std::mem::size_of_val(idx));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, ib, glow::STATIC_DRAW);
    let stride = std::mem::size_of::<loader_vert::V>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 24);
    gl.enable_vertex_attrib_array(3);
    gl.vertex_attrib_pointer_f32(3, 4, glow::UNSIGNED_BYTE, true, stride, 32);
    gl.enable_vertex_attrib_array(4);
    gl.vertex_attrib_pointer_f32(4, 2, glow::FLOAT, false, stride, 36);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
    Ok(GpuMesh { vao, vbo, ibo, parts, bytes: vb.len() + ib.len(), last: 0 })
}

/// Matrice monde (colonnes) : rotation (quaternion) × échelle, puis position.
pub fn model_matrix(pos: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = q;
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)],
        [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)],
        [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [
        r[0][0] * s[0], r[0][1] * s[0], r[0][2] * s[0], 0.0,
        r[1][0] * s[1], r[1][1] * s[1], r[1][2] * s[1], 0.0,
        r[2][0] * s[2], r[2][1] * s[2], r[2][2] * s[2], 0.0,
        pos[0], pos[1], pos[2], 1.0,
    ]
}
