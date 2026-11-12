//! Éditeur de terrain « façon Blender » : on part d'une carte vide (la carte du jeu est cachée), on crée un
//! terrain (sable, terre…) et on le sculpte avec des pinceaux : monter, creuser, pelleteuse (aplanir),
//! lisser, peindre (sable, terre, herbe, roche, béton, goudron), tracer une route, faire une île.
//! Le terrain est dessiné par la carte graphique comme un modèle et s'exporte pour Sollumz / Blender.

use crate::app::DwApp;
use crate::gta::drawable::{RGeom, RModel, RVert, TexData, TexFmt};
use crate::gta::loader::Loaded;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, PointerButton, Pos2, RichText};
use serde::{Deserialize, Serialize};

/// Matériaux du terrain : nom et couleur de base.
pub const MATERIALS: [(&str, [u8; 3]); 7] = [
    ("Sable", [214, 196, 150]),
    ("Terre", [128, 96, 66]),
    ("Herbe", [92, 128, 64]),
    ("Roche", [118, 116, 110]),
    ("Béton", [170, 170, 165]),
    ("Goudron", [52, 54, 58]),
    ("Route", [64, 66, 70]),
];
const ROAD: u8 = 6;

/// Dictionnaire de textures partagé par toutes les tuiles du terrain exporté.
pub const TERRAIN_TXD: &str = "dw_terrain_txd";
/// Distance d'affichage des tuiles exportées (m) — **estimé** (la map désert utilise 1800 à 5000).
pub const TERRAIN_LOD_DIST: f32 = 3000.0;
/// Drapeaux du type d'objet et du placement des tuiles : relevés sur desert_map.ytyp.xml / desert_map.ymap.xml
/// (E:\DESERT-MAP, map validée en jeu).
pub const TERRAIN_ARCH_FLAGS: u32 = 538968064;
pub const TERRAIN_ENTITY_FLAGS: u32 = 1572864;

/// Nom du modèle du terrain pour la carte graphique.
pub const TERRAIN_ARCH: &str = "dw_terrain_live";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Terrain {
    /// coin bas-gauche (x, y) et hauteur de départ
    pub origin: V3,
    pub cell: f32,
    pub nx: usize,
    pub ny: usize,
    /// hauteur de chaque sommet (nx × ny)
    pub h: std::sync::Arc<Vec<f32>>,
    /// matériau de chaque case ((nx-1) × (ny-1))
    pub mat: std::sync::Arc<Vec<u8>>,
    /// routes tracées sur le terrain
    #[serde(default)]
    pub roads: Vec<crate::roads::Road>,
    /// carrefours entre routes
    #[serde(default)]
    pub junctions: Vec<crate::roads::Junction>,
    /// voitures de PNJ qui roulent sur les routes (script)
    #[serde(default)]
    pub npc_traffic: crate::traffic::NpcTraffic,
    /// feux rouges et stops posés tout seuls aux carrefours (signals.rs)
    #[serde(default = "crate::signals::yes")]
    pub auto_signals: bool,
    /// zones d'eau (comme les WaterQuads du jeu)
    #[serde(default)]
    pub waters: Vec<crate::scenery::WaterZone>,
    /// effets de particules posés
    #[serde(default)]
    pub emitters: Vec<crate::scenery::Emitter>,
    /// herbe du jeu de chaque case ((nx-1) × (ny-1)) : 0 aucune, sinon 1 + numéro dans grass::KINDS ; à l'export,
    /// la collision de la case porte l'étiquette de végétation procédurale correspondante (le jeu fait pousser
    /// l'herbe lui-même)
    #[serde(default)]
    pub grass: std::sync::Arc<Vec<u8>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Brush {
    Raise,
    Lower,
    Flatten,
    Smooth,
    /// relief naturel : ajoute des bosses irrégulières
    Noise,
    Paint,
    Road,
    /// arbres, buissons, rochers du jeu
    Vegetation,
    /// zones d'eau (rectangle)
    Water,
    /// effets de particules du jeu
    Particles,
}

/// Modèles de départ d'un terrain : nom, description, style de peinture (0 verdoyant, 1 désert).
pub const PRESETS: [(&str, &str, u8); 9] = [
    ("Plat", "Un sol plat, à la hauteur choisie", 0),
    ("Collines", "Collines douces et herbeuses", 0),
    ("Montagnes", "Crêtes rocheuses et vallées profondes", 0),
    ("Dunes", "Désert de dunes en vagues, sable fin", 1),
    ("Île", "Une grande île au milieu de la mer, plages tout autour", 0),
    ("Archipel", "Plusieurs petites îles dans la mer", 0),
    ("Canyon", "Plateau désertique coupé par un canyon sinueux", 1),
    ("Vallée et rivière", "Deux versants et une rivière au fond (sous le niveau de la mer)", 0),
    ("Plateaux", "Mesas en terrasses, façon désert du Nevada", 1),
];

pub struct TerrainState {
    pub brush: Brush,
    pub radius: f32,
    pub strength: f32,
    pub paint: u8,
    pub new_size: f32,
    pub new_mat: u8,
    pub new_height: f32,
    /// hauteur visée par la pelleteuse (prise au début du coup de pinceau)
    pub flat_target: Option<f32>,
    pub dirty: bool,
    pub generation: u64,
    pub sent: u64,
    /// partie de l'onglet : 0 Créer, 1 Sculpter, 2 Peindre, 3 Routes, 4 Végétation, 5 Exporter
    pub section: u8,
    /// modèle de départ choisi et graine du hasard
    pub preset: u8,
    pub seed: u32,
    /// type de route et points déjà cliqués de la route en cours
    pub road_kind: u8,
    pub road_pts: Vec<V3>,
    /// végétation : ensemble choisi, densité (0..1), dernier point semé
    pub veg_set: usize,
    pub veg_density: f32,
    pub veg_last: Option<V3>,
    /// pinceau végétation : 0 objets (arbres, buissons…), 1 herbe du jeu ; herbe choisie (grass::KINDS) ; biome
    pub veg_mode: u8,
    pub grass_kind: u8,
    pub biome: usize,
    /// eau : coin de départ du rectangle, creuser le bassin, profondeur
    pub water_start: Option<V3>,
    pub water_dig: bool,
    pub water_depth: f32,
    /// effet choisi (index dans scenery::FX) et taille
    pub fx: usize,
    pub fx_scale: f32,
}

impl Default for TerrainState {
    fn default() -> Self {
        TerrainState {
            brush: Brush::Raise,
            radius: 12.0,
            strength: 4.0,
            paint: 1,
            new_size: 512.0,
            new_mat: 0,
            new_height: 2.0,
            flat_target: None,
            dirty: true,
            generation: 0,
            sent: u64::MAX,
            section: 0,
            preset: 1,
            seed: 1,
            road_kind: 0,
            road_pts: Vec::new(),
            veg_set: 0,
            veg_density: 0.5,
            veg_last: None,
            veg_mode: 0,
            grass_kind: 0,
            biome: 0,
            water_start: None,
            water_dig: true,
            water_depth: 2.0,
            fx: 0,
            fx_scale: 1.0,
        }
    }
}

/// Clé de la texture d'un matériau du terrain.
pub fn material_key(m: usize) -> u64 {
    0xD7E2_0000_0000_0000u64 | m as u64
}

/// Bruit lisse non répétitif sur le monde (une valeur par mètre / `scale`), 0..1.
fn wnoise(x: f32, y: f32, scale: f32, octaves: u32, seed: u32) -> f32 {
    // période de 4096 cases : aucune répétition visible sur un terrain
    let p = 4096u32;
    let (u, v) = ((x / scale).rem_euclid(p as f32) / p as f32, (y / scale).rem_euclid(p as f32) / p as f32);
    let (mut s, mut amp, mut tot) = (0.0, 1.0, 0.0);
    let (mut uu, mut vv) = (u, v);
    for o in 0..octaves {
        s += vnoise(uu, vv, p, seed.wrapping_add(o * 1013)) * amp;
        tot += amp;
        amp *= 0.5;
        // octave suivante : coordonnées doublées (la période en cases reste la même, donc sans couture)
        uu = (uu * 2.0).fract();
        vv = (vv * 2.0).fract();
    }
    s / tot
}

impl Terrain {
    pub fn new(center: V3, size: f32, base_h: f32, mat: u8) -> Terrain {
        let cell = if size > 600.0 { 4.0 } else { 2.0 };
        let n = ((size / cell).round() as usize + 1).clamp(16, 400);
        let half = (n - 1) as f32 * cell * 0.5;
        Terrain { origin: v3(center.x - half, center.y - half, 0.0), cell, nx: n, ny: n, h: std::sync::Arc::new(vec![base_h; n * n]), mat: std::sync::Arc::new(vec![mat; (n - 1) * (n - 1)]), roads: Vec::new(), junctions: Vec::new(), waters: Vec::new(), emitters: Vec::new(), npc_traffic: Default::default(), auto_signals: true, grass: Default::default() }
    }

    /// Remplace le relief par un modèle de départ (voir PRESETS), puis peint selon le relief.
    pub fn generate(&mut self, preset: u8, seed: u32, base: f32) {
        let size = (self.nx - 1) as f32 * self.cell;
        let c = v3(self.origin.x + size * 0.5, self.origin.y + size * 0.5, 0.0);
        let s = seed.wrapping_mul(7919);
        for j in 0..self.ny {
            for i in 0..self.nx {
                let p = self.vpos(i, j);
                let (x, y) = (p.x, p.y);
                let (lx, ly) = ((x - c.x) / size, (y - c.y) / size);
                let n = |sc: f32, o: u32| wnoise(x, y, sc, o, s);
                let h = match preset {
                    0 => base,
                    1 => base + (n(160.0, 4) - 0.35) * 40.0,
                    2 => {
                        let r = 1.0 - (2.0 * n(260.0, 5) - 1.0).abs();
                        base + r * r * 150.0 + (n(60.0, 3) - 0.5) * 12.0
                    }
                    3 => {
                        let a = 0.6f32;
                        let t = (x * a.cos() + y * a.sin()) / 38.0 + n(140.0, 3) * 4.0;
                        let wave = t.fract();
                        // dune : pente douce au vent, raide sous le vent
                        let prof = if wave < 0.75 { wave / 0.75 } else { (1.0 - wave) / 0.25 };
                        base + prof * (8.0 + n(220.0, 2) * 10.0) + (n(30.0, 2) - 0.5) * 2.0
                    }
                    4 => {
                        let d = (lx * lx + ly * ly).sqrt() / 0.42;
                        -10.0 + (1.0 - d * d) * 32.0 + (n(80.0, 4) - 0.5) * 14.0
                    }
                    5 => -7.0 + (n(110.0, 4) - 0.48).max(0.0) * 140.0,
                    6 => {
                        let cx = (ly * 9.0).sin() * 0.14 + (n(300.0, 2) - 0.5) * 0.1;
                        let d = ((lx - cx).abs() * size - 18.0).max(0.0);
                        let wall = (d / 40.0).min(1.0);
                        let wall = wall * wall * (3.0 - 2.0 * wall);
                        base + wall * 45.0 + (n(90.0, 3) - 0.5) * 6.0
                    }
                    7 => {
                        let rx = (ly * 6.0).sin() * 0.08;
                        let d = (lx - rx).abs() * size;
                        let bed = if d < 14.0 { -3.0 } else { -3.0 + ((d - 14.0) / 10.0).min(1.0) * 5.0 };
                        bed + ((d - 24.0).max(0.0) * 0.18).min(70.0) * (0.6 + n(150.0, 3) * 0.8)
                    }
                    _ => {
                        let v = n(200.0, 4);
                        let step = (v * 5.0).floor() / 5.0;
                        let edge = ((v * 5.0).fract() - 0.85).max(0.0) / 0.15;
                        base + (step + edge * 0.2) * 70.0 + (n(25.0, 2) - 0.5) * 1.5
                    }
                };
                std::sync::Arc::make_mut(&mut self.h)[j * self.nx + i] = h;
            }
        }
        self.auto_paint(PRESETS[(preset as usize).min(PRESETS.len() - 1)].2);
    }

    /// Peint selon le relief : plage au bord de l'eau, roche sur les pentes raides et en altitude, terre sur
    /// les pentes moyennes, herbe (ou sable dans le style désert) ailleurs.
    pub fn auto_paint(&mut self, style: u8) {
        let (nx, ny) = (self.nx, self.ny);
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let h = |a: usize, b: usize| self.h[b * nx + a];
                let z = (h(i, j) + h(i + 1, j) + h(i, j + 1) + h(i + 1, j + 1)) * 0.25;
                let sx = ((h(i + 1, j) + h(i + 1, j + 1)) - (h(i, j) + h(i, j + 1))) * 0.5 / self.cell;
                let sy = ((h(i, j + 1) + h(i + 1, j + 1)) - (h(i, j) + h(i + 1, j))) * 0.5 / self.cell;
                let slope = (sx * sx + sy * sy).sqrt();
                let m = &mut std::sync::Arc::make_mut(&mut self.mat)[j * (nx - 1) + i];
                if *m >= 4 {
                    continue; // béton, goudron et routes gardés
                }
                *m = if slope > 0.8 || z > 90.0 {
                    3
                } else if z < 1.6 {
                    0
                } else if slope > 0.4 {
                    if style == 1 { 3 } else { 1 }
                } else if style == 1 {
                    if slope > 0.2 { 1 } else { 0 }
                } else {
                    2
                };
            }
        }
    }

    /// Aplanit le terrain sous une route (chaussée et trottoirs à sa hauteur, raccord en pente douce sur 8 m
    /// de chaque côté) et peint le dessous en goudron (ou en terre pour un chemin).
    pub fn apply_road(&mut self, r: &crate::roads::Road) {
        let half = r.half_total();
        let blend = 8.0;
        let reach = half + blend;
        let (nx, ny) = (self.nx, self.ny);
        let mut best = vec![(f32::MAX, 0.0f32); nx * ny];
        for w in r.samples.windows(2) {
            let (a, b) = (w[0], w[1]);
            let i0 = (((a[0].min(b[0]) - reach - self.origin.x) / self.cell).floor().max(0.0)) as usize;
            let j0 = (((a[1].min(b[1]) - reach - self.origin.y) / self.cell).floor().max(0.0)) as usize;
            let i1 = ((((a[0].max(b[0]) + reach - self.origin.x) / self.cell).ceil().max(0.0)) as usize).min(nx - 1);
            let j1 = ((((a[1].max(b[1]) + reach - self.origin.y) / self.cell).ceil().max(0.0)) as usize).min(ny - 1);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = (dx * dx + dy * dy).max(1e-6);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let p = self.vpos(i, j);
                    let t = (((p.x - a[0]) * dx + (p.y - a[1]) * dy) / l2).clamp(0.0, 1.0);
                    let d = ((p.x - a[0] - dx * t).powi(2) + (p.y - a[1] - dy * t).powi(2)).sqrt();
                    let k = j * nx + i;
                    if d < best[k].0 {
                        best[k] = (d, a[2] + (b[2] - a[2]) * t);
                    }
                }
            }
        }
        for (k, (d, z)) in best.iter().enumerate() {
            if *d > reach {
                continue;
            }
            let target = z - 0.05;
            let w = if *d <= half + 0.5 { 1.0 } else { let t = 1.0 - (*d - half - 0.5) / (blend - 0.5); t * t * (3.0 - 2.0 * t) };
            let v = self.h[k];
            std::sync::Arc::make_mut(&mut self.h)[k] = v + (target - v) * w;
        }
        let m = if r.kind().dirt { 1 } else { ROAD };
        let hw = r.kind().width * 0.5;
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let x = self.origin.x + (i as f32 + 0.5) * self.cell;
                let y = self.origin.y + (j as f32 + 0.5) * self.cell;
                let k = j * nx + i;
                if best[k].0 > reach {
                    continue;
                }
                if let Some((d, _)) = r.nearest(x, y) {
                    if d <= hw {
                        std::sync::Arc::make_mut(&mut self.mat)[j * (nx - 1) + i] = m;
                    }
                }
            }
        }
    }

    /// Morceau du terrain (sommets i0..=i1, j0..=j1) comme un terrain à part, coin bas-gauche décalé.
    pub fn sub(&self, i0: usize, j0: usize, i1: usize, j1: usize) -> Terrain {
        let (nx, ny) = (i1 - i0 + 1, j1 - j0 + 1);
        let mut h = Vec::with_capacity(nx * ny);
        for j in j0..=j1 {
            h.extend_from_slice(&self.h[j * self.nx + i0..=j * self.nx + i1]);
        }
        let mut mat = Vec::with_capacity((nx - 1) * (ny - 1));
        let mut grass = Vec::with_capacity((nx - 1) * (ny - 1));
        for j in j0..j1 {
            mat.extend_from_slice(&self.mat[j * (self.nx - 1) + i0..j * (self.nx - 1) + i1]);
            for i in i0..i1 {
                grass.push(self.grass_at(i, j));
            }
        }
        Terrain { origin: v3(self.origin.x + i0 as f32 * self.cell, self.origin.y + j0 as f32 * self.cell, self.origin.z), cell: self.cell, nx, ny, h: h.into(), mat: mat.into(), roads: Vec::new(), junctions: Vec::new(), waters: Vec::new(), emitters: Vec::new(), npc_traffic: Default::default(), auto_signals: true, grass: grass.into() }
    }

    /// Version simplifiée pour la vue de loin : un sommet sur `f` dans chaque direction (cases `f` fois plus
    /// grandes), hauteurs reprises du terrain (bord de tuile compris), matériau de la case du coin.
    pub fn downsample(&self, f: usize) -> Terrain {
        let f = f.max(1);
        let nx = (self.nx - 1).div_ceil(f) + 1;
        let ny = (self.ny - 1).div_ceil(f) + 1;
        let cell = self.cell * f as f32;
        let gi = |i: usize| (i * f).min(self.nx - 1);
        let gj = |j: usize| (j * f).min(self.ny - 1);
        let mut h = Vec::with_capacity(nx * ny);
        for j in 0..ny {
            for i in 0..nx {
                h.push(self.h[gj(j) * self.nx + gi(i)]);
            }
        }
        let mut mat = Vec::with_capacity((nx - 1) * (ny - 1));
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                mat.push(self.mat[gj(j).min(self.ny - 2) * (self.nx - 1) + gi(i).min(self.nx - 2)]);
            }
        }
        Terrain { origin: self.origin, cell, nx, ny, h: h.into(), mat: mat.into(), roads: Vec::new(), junctions: Vec::new(), waters: Vec::new(), emitters: Vec::new(), npc_traffic: Default::default(), auto_signals: true, grass: Default::default() }
    }

    /// Herbe du jeu d'une case (0 = aucune).
    pub fn grass_at(&self, i: usize, j: usize) -> u8 {
        self.grass.get(j * (self.nx - 1) + i).copied().unwrap_or(0)
    }

    /// Peint l'herbe du jeu `k` (0 = l'enlève) dans le disque ; jamais sur le béton, le goudron ni les routes
    /// (matériaux 4 et plus). Rend le nombre de cases changées.
    pub fn paint_grass(&mut self, c: V3, r: f32, k: u8) -> usize {
        let n = (self.nx - 1) * (self.ny - 1);
        if self.grass.len() != n {
            self.grass = std::sync::Arc::new(vec![0; n]);
        }
        let mut changed = 0;
        for j in 0..self.ny - 1 {
            for i in 0..self.nx - 1 {
                let x = self.origin.x + (i as f32 + 0.5) * self.cell;
                let y = self.origin.y + (j as f32 + 0.5) * self.cell;
                if (x - c.x).powi(2) + (y - c.y).powi(2) > r * r {
                    continue;
                }
                let idx = j * (self.nx - 1) + i;
                let v = if k > 0 && self.mat[idx] >= 4 { 0 } else { k };
                if self.grass[idx] != v {
                    std::sync::Arc::make_mut(&mut self.grass)[idx] = v;
                    changed += 1;
                }
            }
        }
        changed
    }

    fn vpos(&self, i: usize, j: usize) -> V3 {
        v3(self.origin.x + i as f32 * self.cell, self.origin.y + j as f32 * self.cell, self.h[j * self.nx + i])
    }

    /// Hauteur du terrain en (x, y) (interpolée), si le point est dessus.
    pub fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        let fx = (x - self.origin.x) / self.cell;
        let fy = (y - self.origin.y) / self.cell;
        if fx < 0.0 || fy < 0.0 || fx >= (self.nx - 1) as f32 || fy >= (self.ny - 1) as f32 {
            return None;
        }
        let (i, j) = (fx as usize, fy as usize);
        let (tx, ty) = (fx - i as f32, fy - j as f32);
        let h = |a: usize, b: usize| self.h[b * self.nx + a];
        Some(h(i, j) * (1.0 - tx) * (1.0 - ty) + h(i + 1, j) * tx * (1.0 - ty) + h(i, j + 1) * (1.0 - tx) * ty + h(i + 1, j + 1) * tx * ty)
    }

    /// Matériau de la case sous (x, y).
    pub fn material_at(&self, x: f32, y: f32) -> Option<u8> {
        let fx = (x - self.origin.x) / self.cell;
        let fy = (y - self.origin.y) / self.cell;
        if fx < 0.0 || fy < 0.0 || fx >= (self.nx - 1) as f32 || fy >= (self.ny - 1) as f32 {
            return None;
        }
        self.mat.get(fy as usize * (self.nx - 1) + fx as usize).copied()
    }

    pub fn bounds(&self) -> (V3, V3) {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in self.h.iter() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        for r in &self.roads {
            for s in &r.samples {
                hi = hi.max(s[2] + 0.3);
                lo = lo.min(s[2] - 0.7);
            }
        }
        (v3(self.origin.x, self.origin.y, lo), v3(self.origin.x + (self.nx - 1) as f32 * self.cell, self.origin.y + (self.ny - 1) as f32 * self.cell, hi))
    }

    /// Applique une fonction aux sommets dans le rayon, avec le poids (1 au centre, 0 au bord).
    fn each_vertex(&mut self, c: V3, r: f32, mut f: impl FnMut(&mut Terrain, usize, usize, f32)) {
        let i0 = (((c.x - r - self.origin.x) / self.cell).floor().max(0.0)) as usize;
        let j0 = (((c.y - r - self.origin.y) / self.cell).floor().max(0.0)) as usize;
        let i1 = ((((c.x + r - self.origin.x) / self.cell).ceil()) as usize).min(self.nx - 1);
        let j1 = ((((c.y + r - self.origin.y) / self.cell).ceil()) as usize).min(self.ny - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let p = self.vpos(i, j);
                let d = ((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt();
                if d <= r {
                    let w = 1.0 - d / r;
                    f(self, i, j, w * w * (3.0 - 2.0 * w));
                }
            }
        }
    }

    pub fn brush(&mut self, b: Brush, c: V3, r: f32, strength: f32, dt: f32, paint: u8, target: f32) {
        match b {
            Brush::Raise | Brush::Lower => {
                let s = if b == Brush::Raise { 1.0 } else { -1.0 } * strength * dt;
                self.each_vertex(c, r, |t, i, j, w| std::sync::Arc::make_mut(&mut t.h)[j * t.nx + i] += s * w);
            }
            Brush::Flatten => {
                let k = (strength * dt * 1.5).min(1.0);
                self.each_vertex(c, r, |t, i, j, w| {
                    let v = &mut std::sync::Arc::make_mut(&mut t.h)[j * t.nx + i];
                    *v += (target - *v) * (k * w * 2.0).min(1.0);
                });
            }
            Brush::Smooth => {
                let k = (strength * dt * 1.5).min(1.0);
                let old = self.h.clone();
                let (nx, ny) = (self.nx, self.ny);
                self.each_vertex(c, r, |t, i, j, w| {
                    let g = |a: usize, b: usize| old[b * nx + a];
                    let avg = (g(i.saturating_sub(1), j) + g((i + 1).min(nx - 1), j) + g(i, j.saturating_sub(1)) + g(i, (j + 1).min(ny - 1))) * 0.25;
                    let v = &mut std::sync::Arc::make_mut(&mut t.h)[j * nx + i];
                    *v += (avg - *v) * (k * w * 2.0).min(1.0);
                });
            }
            Brush::Noise => {
                let s = strength * dt * 1.5;
                let seed = 4242;
                self.each_vertex(c, r, |t, i, j, w| {
                    let p = t.vpos(i, j);
                    let n = wnoise(p.x, p.y, 9.0, 3, seed) - 0.5;
                    std::sync::Arc::make_mut(&mut t.h)[j * t.nx + i] += n * s * w;
                });
            }
            Brush::Paint => self.paint_disc(c, r, paint),
            Brush::Road | Brush::Vegetation | Brush::Water | Brush::Particles => {}
        }
    }

    fn paint_disc(&mut self, c: V3, r: f32, m: u8) {
        for j in 0..self.ny - 1 {
            for i in 0..self.nx - 1 {
                let x = self.origin.x + (i as f32 + 0.5) * self.cell;
                let y = self.origin.y + (j as f32 + 0.5) * self.cell;
                if (x - c.x).abs() > r || (y - c.y).abs() > r {
                    continue;
                }
                if ((x - c.x).powi(2) + (y - c.y).powi(2)).sqrt() <= r {
                    std::sync::Arc::make_mut(&mut self.mat)[j * (self.nx - 1) + i] = m;
                }
            }
        }
    }

    /// Route de `a` à `b` : bande de goudron de largeur `w`, aplanie en pente douce d'un bout à l'autre, bords raccordés.
    pub fn road(&mut self, a: V3, b: V3, w: f32) {
        let ha = self.height_at(a.x, a.y).unwrap_or(a.z);
        let hb = self.height_at(b.x, b.y).unwrap_or(b.z);
        let ab = v3(b.x - a.x, b.y - a.y, 0.0);
        let l2 = (ab.x * ab.x + ab.y * ab.y).max(1e-3);
        let seg = |x: f32, y: f32| -> (f32, f32) {
            let t = (((x - a.x) * ab.x + (y - a.y) * ab.y) / l2).clamp(0.0, 1.0);
            let (px, py) = (a.x + ab.x * t, a.y + ab.y * t);
            (t, ((x - px).powi(2) + (y - py).powi(2)).sqrt())
        };
        let half = w * 0.5;
        let edge = w * 0.6;
        for j in 0..self.ny {
            for i in 0..self.nx {
                let p = self.vpos(i, j);
                let (t, d) = seg(p.x, p.y);
                if d > half + edge {
                    continue;
                }
                let target = ha + (hb - ha) * t;
                let k = if d <= half { 1.0 } else { 1.0 - (d - half) / edge };
                let v = &mut std::sync::Arc::make_mut(&mut self.h)[j * self.nx + i];
                *v += (target - *v) * k;
            }
        }
        for j in 0..self.ny - 1 {
            for i in 0..self.nx - 1 {
                let x = self.origin.x + (i as f32 + 0.5) * self.cell;
                let y = self.origin.y + (j as f32 + 0.5) * self.cell;
                if seg(x, y).1 <= half {
                    std::sync::Arc::make_mut(&mut self.mat)[j * (self.nx - 1) + i] = ROAD;
                }
            }
        }
    }

    /// Île ronde : colline de sable au-dessus de la mer, plage en pente douce.
    pub fn island(&mut self, c: V3, r: f32, top: f32) {
        for j in 0..self.ny {
            for i in 0..self.nx {
                let p = self.vpos(i, j);
                let d = ((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt() / r;
                if d < 1.2 {
                    let hh = top * (1.0 - d * d).max(-0.2);
                    let v = &mut std::sync::Arc::make_mut(&mut self.h)[j * self.nx + i];
                    *v = v.max(hh);
                }
            }
        }
        self.paint_disc(c, r * 1.05, 0);
        self.paint_disc(c, r * 0.7, 2);
    }

    /// Modèle à dessiner : une partie par matériau, une petite texture de couleur par matériau.
    pub fn model(&self) -> (RModel, Vec<u64>, Vec<(u64, TexData)>) {
        let (nx, ny) = (self.nx, self.ny);
        let mut verts = Vec::with_capacity(nx * ny);
        // coordonnées de texture alignées sur le monde (tous les 6 m) : pas de couture entre les tuiles
        let (ox, oy) = (self.origin.x.rem_euclid(6.0), self.origin.y.rem_euclid(6.0));
        for j in 0..ny {
            for i in 0..nx {
                let g = |a: usize, b: usize| self.h[b.min(ny - 1) * nx + a.min(nx - 1)];
                let dx = (g(i + 1, j) - g(i.saturating_sub(1), j)) / (2.0 * self.cell);
                let dy = (g(i, j + 1) - g(i, j.saturating_sub(1))) / (2.0 * self.cell);
                let n = v3(-dx, -dy, 1.0).norm();
                let x = i as f32 * self.cell;
                let y = j as f32 * self.cell;
                verts.push(RVert { pos: [x, y, self.h[j * nx + i]], nrm: [n.x, n.y, n.z], uv: [(ox + x) / 6.0, (oy + y) / 6.0], col: [255; 4], uv1: [0.0, 0.0] });
            }
        }
        let mut per: Vec<Vec<u32>> = vec![Vec::new(); MATERIALS.len()];
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let m = (self.mat[j * (nx - 1) + i] as usize).min(MATERIALS.len() - 1);
                let a = (j * nx + i) as u32;
                let (b, c, d) = (a + 1, a + nx as u32, a + nx as u32 + 1);
                per[m].extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        let mut idx = Vec::new();
        let mut geoms = Vec::new();
        let mut keys = Vec::new();
        let mut texs = Vec::new();
        for (m, list) in per.into_iter().enumerate() {
            let key = material_key(m);
            texs.push((key, material_texture(m)));
            // morceaux de 60 000 indices au plus : chaque partie du modèle du jeu compte ses sommets sur 16 bits
            for chunk in list.chunks(60000) {
                geoms.push(RGeom { first: idx.len() as u32, count: chunk.len() as u32, tex: m as u32, cutout: false, blend: 0, glass: false, ..Default::default() });
                keys.push(key);
                idx.extend_from_slice(chunk);
            }
        }
        // routes : mêmes sommets / indices que le terrain, textures à la suite
        if !self.roads.is_empty() {
            texs.extend(crate::roads::road_textures());
            for r in &self.roads {
                let (pos, uv, parts) = r.mesh(self.origin);
                append_mesh(&mut verts, &mut idx, &mut geoms, &mut keys, &texs, &pos, &uv, &parts);
            }
            for j in &self.junctions {
                let (pos, uv, parts) = crate::roads::junction_mesh(j, self.origin);
                append_mesh(&mut verts, &mut idx, &mut geoms, &mut keys, &texs, &pos, &uv, &parts);
            }
        }
        // vraies textures du jeu à la place de celles de DragoWalker (si le jeu est chargé et le réglage actif)
        crate::groundtex::apply(&mut texs);
        (RModel { verts, idx, geoms, embedded_dict: None }, keys, texs)
    }
}

/// Ajoute un maillage (positions, coordonnées de texture, parties) à un modèle : normales calculées depuis
/// les triangles, une partie par texture (≤ 60 000 indices chacune).
#[allow(clippy::too_many_arguments)]
pub fn append_mesh(verts: &mut Vec<RVert>, idx: &mut Vec<u32>, geoms: &mut Vec<RGeom>, keys: &mut Vec<u64>, texs: &[(u64, TexData)], pos: &[V3], uv: &[[f32; 2]], parts: &[(u64, Vec<u32>, u8)]) {
    let base = verts.len() as u32;
    let mut nrm = vec![V3::ZERO; pos.len()];
    for (_, list, _) in parts {
        for t in list.chunks_exact(3) {
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let n = (b - a).cross(c - a);
            for k in t {
                nrm[*k as usize] = nrm[*k as usize] + n;
            }
        }
    }
    for (i, p) in pos.iter().enumerate() {
        let n = if nrm[i].len() > 1e-6 { nrm[i].norm() } else { v3(0.0, 0.0, 1.0) };
        verts.push(RVert { pos: [p.x, p.y, p.z], nrm: [n.x, n.y, n.z], uv: uv[i], col: [255; 4], uv1: [0.0, 0.0] });
    }
    for (key, list, _) in parts {
        let tex = texs.iter().position(|t| t.0 == *key).unwrap_or(0) as u32;
        for chunk in list.chunks(60000) {
            geoms.push(RGeom { first: idx.len() as u32, count: chunk.len() as u32, tex, cutout: false, blend: 0, glass: false, ..Default::default() });
            keys.push(*key);
            idx.extend(chunk.iter().map(|i| i + base));
        }
    }
}

/// Taille des textures du terrain (carrées, qui se répètent sans couture tous les 6 m).
const TEX_SIZE: u32 = 256;

/// Texture d'un matériau, pour les aperçus.
pub fn material_preview_texture(m: usize) -> TexData {
    material_texture(m.min(MATERIALS.len() - 1))
}

/// Texture d'un matériau (gardée en mémoire : la même pour la vue et pour l'export).
fn material_texture(m: usize) -> TexData {
    static CACHE: std::sync::OnceLock<Vec<TexData>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| (0..MATERIALS.len()).map(make_material_texture).collect())[m].clone()
}

fn hash2(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Bruit lisse qui se répète (période `p` cases) : la texture se raccorde à elle-même.
fn vnoise(u: f32, v: f32, p: u32, seed: u32) -> f32 {
    let (x, y) = (u * p as f32, v * p as f32);
    let (xi, yi) = (x.floor() as i64, y.floor() as i64);
    let (tx, ty) = (x - xi as f32, y - yi as f32);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let w = |a: i64| a.rem_euclid(p as i64) as u32;
    let g = |a: i64, b: i64| hash2(w(a), w(b), seed);
    let a = g(xi, yi) + (g(xi + 1, yi) - g(xi, yi)) * sx;
    let b = g(xi, yi + 1) + (g(xi + 1, yi + 1) - g(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

/// Somme de bruits de plus en plus fins (0..1).
fn fbm(u: f32, v: f32, base: u32, octaves: u32, seed: u32) -> f32 {
    let (mut s, mut amp, mut tot, mut p) = (0.0, 1.0, 0.0, base);
    for o in 0..octaves {
        s += vnoise(u, v, p, seed.wrapping_add(o * 977)) * amp;
        tot += amp;
        amp *= 0.5;
        p *= 2;
    }
    s / tot
}

/// Texture 256 × 256 d'un matériau, avec tous ses niveaux de détail (sans eux, le jeu affiche du noir ou
/// du gris de loin) : sable à grain fin et rides, terre craquelée, herbe touffue, roche veinée, béton, goudron.
fn make_material_texture(m: usize) -> TexData {
    let n = TEX_SIZE;
    let base = MATERIALS[m].1;
    let seed = 0x51ED_0000 + m as u32 * 131;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let grain = hash2(x, y, seed) - 0.5;
            let large = fbm(u, v, 4, 4, seed) - 0.5;
            let (shade, tint) = match m {
                // sable : rides dans une direction, grain fin
                0 => {
                    let ripple = ((u * 6.0 + large * 0.6) * std::f32::consts::TAU * 2.0).sin() * 0.04;
                    (1.0 + large * 0.18 + ripple + grain * 0.10, [0.0, 0.0, 0.0])
                }
                // terre : taches sombres et fissures
                1 => {
                    let cr = (fbm(u, v, 8, 3, seed ^ 7) - 0.5).abs();
                    let crack = if cr < 0.025 { -0.30 } else { 0.0 };
                    (1.0 + large * 0.30 + grain * 0.14 + crack, [0.0, 0.0, 0.0])
                }
                // herbe : touffes plus claires / plus sombres, nuances jaunes
                2 => {
                    let tuft = fbm(u, v, 16, 3, seed ^ 3) - 0.5;
                    (1.0 + tuft * 0.45 + grain * 0.22, [large * 30.0, large * 10.0, 0.0])
                }
                // roche : veines et grosses plaques
                3 => {
                    let vein = ((v * 10.0 + large * 3.0) * std::f32::consts::TAU).sin() * 0.06;
                    (1.0 + large * 0.35 + vein + grain * 0.12, [0.0, 0.0, 0.0])
                }
                // béton : léger, joints tous les quarts
                4 => {
                    let joint = if x % (n / 2) < 2 || y % (n / 2) < 2 { -0.18 } else { 0.0 };
                    (1.0 + large * 0.10 + grain * 0.08 + joint, [0.0, 0.0, 0.0])
                }
                // goudron et route : gravillons
                _ => {
                    let peb = if hash2(x, y, seed ^ 99) > 0.93 { 0.35 } else { 0.0 };
                    (1.0 + large * 0.12 + grain * 0.18 + peb, [0.0, 0.0, 0.0])
                }
            };
            let c = |i: usize| (base[i] as f32 * shade + tint[i]).clamp(0.0, 255.0) as u8;
            d.extend_from_slice(&[c(0), c(1), c(2), 255]);
        }
    }
    // niveaux de détail : moyenne de 2 × 2 pixels jusqu'à 1 × 1
    let mut levels = vec![d];
    let mut s = n as usize;
    while s > 1 {
        let prev = levels.last().unwrap();
        let h = s / 2;
        let mut o = Vec::with_capacity(h * h * 4);
        for y in 0..h {
            for x in 0..h {
                for k in 0..4 {
                    let p = |xx: usize, yy: usize| prev[(yy * s + xx) * 4 + k] as u32;
                    o.push(((p(2 * x, 2 * y) + p(2 * x + 1, 2 * y) + p(2 * x, 2 * y + 1) + p(2 * x + 1, 2 * y + 1)) / 4) as u8);
                }
            }
        }
        levels.push(o);
        s = h;
    }
    TexData { w: n, h: n, fmt: TexFmt::Rgba8, levels }
}

impl DwApp {
    /// Renvoie le terrain à la carte graphique quand il a changé (au plus ~12 fois par seconde).
    pub fn terrain_tick(&mut self, now: f64) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        // terrain absent de la carte graphique (jamais envoyé, ou perdu) : renvoyé, sinon il serait dessiné comme une
        // boîte beige
        if !self.terrain.dirty {
            let arch = crate::gta::crypto::joaat(TERRAIN_ARCH);
            let missing = self.game.gpu.as_ref().and_then(|g| g.lock().ok().map(|g| !g.has_or_loading(arch))).unwrap_or(false);
            // (au plus toutes les 2 s, si l'envoi échouait)
            if !missing || now - self.terrain_last_upload < 2.0 {
                return;
            }
            self.terrain.dirty = true;
        }
        if now - self.terrain_last_upload < 0.08 {
            return;
        }
        let Some(gpu) = &self.game.gpu else { return };
        let (model, part_keys, textures) = t.model();
        if let Ok(mut g) = gpu.lock() {
            g.add_custom(Loaded { arch: crate::gta::crypto::joaat(TERRAIN_ARCH), model: Some(model), part_keys, layer_keys: Vec::new(), textures });
        }
        self.terrain.dirty = false;
        self.terrain.generation += 1;
        self.terrain.sent = self.terrain.generation;
        self.terrain_last_upload = now;
    }

    /// Outil Terrain : pinceaux sous la souris.
    pub fn terrain_tool(&mut self, resp: &egui::Response, ptr: Option<Pos2>, ctx: &egui::Context) {
        let Some(p) = ptr else { return };
        let g = self.terrain_ground(p);
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        if self.proj.terrain.is_none() {
            return;
        }
        // routes : un clic par point, double-clic ou Entrée pour finir
        if self.terrain.brush == Brush::Road {
            if resp.double_clicked_by(PointerButton::Primary) {
                self.finish_road();
                return;
            }
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(p) = g {
                    let p = self.road_snap(p).map(|s| s.0).unwrap_or(p);
                    // comme les liens entre nœuds de route du jeu : longueur gardée sur un octet, 255 m au plus
                    // (YndLink::UpdateLength 0x06002f24) ; plus loin, CodeWalker refuse la liaison
                    // (WorldForm::TryCreateNodeLink 0x06000326 : « nodes are likely too far away »)
                    if let Some(last) = self.terrain.road_pts.last() {
                        let d = ((p.x - last.x).powi(2) + (p.y - last.y).powi(2)).sqrt();
                        if d > crate::roads::MAX_LINK {
                            self.status = format!("Point trop loin du précédent ({d:.0} m) : {:.0} m au plus entre deux points d'une route, comme les liens de route du jeu.", crate::roads::MAX_LINK);
                            return;
                        }
                    }
                    self.terrain.road_pts.push(p);
                    let n = self.terrain.road_pts.len();
                    self.status = if n == 1 {
                        format!("{} : premier point posé. Clique les points suivants · double-clic ou Entrée pour finir · Retour arrière : enlever le dernier · Échap : annuler.", crate::roads::KINDS[self.terrain.road_kind as usize].name)
                    } else {
                        format!("{n} points. Double-clic ou Entrée pour construire la route.")
                    };
                }
            }
            return;
        }
        if self.terrain.brush == Brush::Water {
            self.water_tool(resp, g);
            return;
        }
        if self.terrain.brush == Brush::Particles {
            let ctrl = ctx.input(|i| i.modifiers.command);
            self.fx_tool(resp, g, ctrl);
            return;
        }
        if resp.drag_started_by(PointerButton::Primary) || resp.clicked_by(PointerButton::Primary) {
            self.push_undo_terrain();
        }
        let active = resp.dragged_by(PointerButton::Primary) || resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down());
        if resp.drag_stopped() {
            self.terrain.flat_target = None;
            self.terrain.veg_last = None;
        }
        if !active {
            return;
        }
        let Some(c) = g else { return };
        // végétation : semer en avançant (tous les 40 % du rayon), Ctrl pour retirer
        if self.terrain.brush == Brush::Vegetation {
            let r = self.terrain.radius;
            if self.terrain.veg_last.map(|l| (l - c).len() < r * 0.4).unwrap_or(false) && !resp.clicked_by(PointerButton::Primary) {
                return;
            }
            self.terrain.veg_last = Some(c);
            // herbe du jeu : une étiquette par case (Ctrl : l'enlève)
            if self.terrain.veg_mode == 1 {
                let k = if ctx.input(|i| i.modifiers.command) { 0 } else { self.terrain.grass_kind + 1 };
                let n = self.proj.terrain.as_mut().map(|t| t.paint_grass(c, r, k)).unwrap_or(0);
                let name = crate::grass::KINDS[(self.terrain.grass_kind as usize).min(crate::grass::KINDS.len() - 1)].name;
                self.status = if k == 0 { format!("Herbe enlevée sur {n} case(s).") } else { format!("Herbe « {name} » sur {n} case(s) · le jeu la fera pousser · Ctrl + pinceau pour l'enlever.") };
                return;
            }
            if ctx.input(|i| i.modifiers.command) {
                let n = self.veg_remove(c, r);
                if n > 0 {
                    self.status = format!("{n} objet(s) de végétation retiré(s) · {} au total.", self.veg_count());
                }
            } else {
                let n = self.veg_paint(c, r);
                self.status = format!("{n} objet(s) posé(s) · {} objets de végétation au total · Ctrl + pinceau pour retirer.", self.veg_count());
            }
            return;
        }
        if self.terrain.brush == Brush::Flatten && self.terrain.flat_target.is_none() {
            self.terrain.flat_target = Some(c.z);
        }
        let (b, r, s, pm, tg) = (self.terrain.brush, self.terrain.radius, self.terrain.strength, self.terrain.paint, self.terrain.flat_target.unwrap_or(c.z));
        if let Some(t) = self.proj.terrain.as_mut() {
            t.brush(b, c, r, s, dt, pm, tg);
        }
        self.terrain.dirty = true;
        ctx.request_repaint();
    }

    /// Accroche un point à une route existante : son extrémité (à moins de 8 m), sinon son axe s'il est sur la
    /// route. Renvoie le point et vrai pour une extrémité.
    pub fn road_snap(&self, p: V3) -> Option<(V3, bool)> {
        let t = self.proj.terrain.as_ref()?;
        let mut best: Option<(f32, V3, bool)> = None;
        for r in &t.roads {
            for s in [r.samples.first()?, r.samples.last()?] {
                let d = ((s[0] - p.x).powi(2) + (s[1] - p.y).powi(2)).sqrt();
                if d < crate::roads::SNAP_END.max(r.kind().width * 0.5) && best.map(|b| d < b.0).unwrap_or(true) {
                    best = Some((d, v3(s[0], s[1], s[2]), true));
                }
            }
        }
        if best.is_none() {
            for r in &t.roads {
                if let Some((d, q, _, _)) = r.nearest_point(p.x, p.y) {
                    if d <= r.kind().width * 0.5 && best.map(|b| d < b.0).unwrap_or(true) {
                        best = Some((d, q, false));
                    }
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }

    /// Construit la route en cours (au moins 2 points) : courbe, pente douce, terrain aplani dessous.
    pub fn finish_road(&mut self) {
        let pts = std::mem::take(&mut self.terrain.road_pts);
        if pts.len() < 2 {
            self.status = "Il faut au moins 2 points pour une route.".into();
            self.terrain.road_pts = pts;
            return;
        }
        let kind = self.terrain.road_kind;
        let Some(t) = self.proj.terrain.as_ref() else { return };
        let Some(mut r) = crate::roads::Road::build(kind, &pts, |x, y| t.height_at(x, y)) else { return };
        self.push_undo();
        let len = r.length();
        let mut nj = 0;
        if let Some(t) = self.proj.terrain.as_mut() {
            // reliée aux routes existantes : hauteur raccordée, trottoirs ouverts, carrefours
            let js = r.attach(&mut t.roads);
            nj = js.len();
            t.apply_road(&r);
            t.roads.push(r);
            t.junctions.extend(js);
        }
        // feux et stops des carrefours (reposés pour tous : une nouvelle branche change la signalisation)
        if nj > 0 {
            self.refresh_signals();
        }
        self.terrain.dirty = true;
        self.status = if nj > 0 {
            format!("{} de {:.0} m construite et reliée : {nj} carrefour(s).", crate::roads::KINDS[kind as usize].name, len)
        } else {
            format!("{} de {:.0} m construite (terrain aplani dessous, pente limitée à 12 %).", crate::roads::KINDS[kind as usize].name, len)
        };
    }

    fn push_undo_terrain(&mut self) {
        // le terrain fait partie du projet : il suit l'annulation (Ctrl+Z)
        self.push_undo();
    }

    /// Point du terrain sous la souris (plan du terrain, sinon le sol à 0 m).
    pub fn terrain_ground(&self, p: Pos2) -> Option<V3> {
        let (o, d) = self.cam.ray(p);
        let t = self.proj.terrain.as_ref()?;
        // marche le long du rayon jusqu'à passer sous le terrain
        let mut prev = o;
        let step = (t.cell * 0.5).max(0.5);
        for k in 1..6000 {
            let q = o + d * (k as f32 * step);
            if let Some(h) = t.height_at(q.x, q.y) {
                if q.z <= h {
                    // affiner entre prev et q
                    let mut a = prev;
                    let mut b = q;
                    for _ in 0..12 {
                        let m = (a + b) * 0.5;
                        if t.height_at(m.x, m.y).map(|hh| m.z <= hh).unwrap_or(false) {
                            b = m;
                        } else {
                            a = m;
                        }
                    }
                    return Some(v3(b.x, b.y, t.height_at(b.x, b.y).unwrap_or(b.z)));
                }
            }
            prev = q;
        }
        None
    }

    /// Cercle du pinceau sous la souris.
    pub fn draw_terrain_brush(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        let (Some(p), Some(t)) = (hover, self.proj.terrain.as_ref()) else { return };
        let Some(c) = self.terrain_ground(p) else { return };
        if self.terrain.brush == Brush::Road {
            // aperçu : courbe passant par les points cliqués et la souris, à la largeur de la route
            let snap = self.road_snap(c);
            let c = snap.map(|s| s.0).unwrap_or(c);
            if let Some((q, end)) = snap {
                if let Some((sp, _)) = self.cam.project(q + v3(0.0, 0.0, 0.4)) {
                    painter.circle_stroke(sp, 9.0, egui::Stroke::new(2.5_f32, if end { Color32::from_rgb(90, 220, 255) } else { Color32::from_rgb(120, 255, 140) }));
                    painter.text(sp + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, if end { "relier au bout" } else { "carrefour" }, egui::FontId::proportional(12.0), Color32::WHITE);
                }
            }
            let mut pts: Vec<[f32; 2]> = self.terrain.road_pts.iter().map(|p| [p.x, p.y]).collect();
            pts.push([c.x, c.y]);
            let hw = crate::roads::KINDS[self.terrain.road_kind as usize].width * 0.5;
            let curve = crate::roads::spline(&pts, 2.0);
            let proj = |x: f32, y: f32| self.cam.project(v3(x, y, t.height_at(x, y).unwrap_or(c.z) + 0.3)).map(|q| q.0);
            for side in [-hw, 0.0, hw] {
                let line: Vec<Pos2> = curve
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        let a = curve[i.saturating_sub(1)];
                        let b = curve[(i + 1).min(curve.len() - 1)];
                        let d = v3(b[0] - a[0], b[1] - a[1], 0.0).norm();
                        proj(p[0] + d.y * side, p[1] - d.x * side)
                    })
                    .collect();
                let col = if side == 0.0 { Color32::from_rgb(255, 210, 60) } else { Color32::from_rgb(240, 240, 240) };
                painter.add(egui::Shape::line(line, egui::Stroke::new(if side == 0.0 { 2.0_f32 } else { 1.5_f32 }, col)));
            }
            for p in &self.terrain.road_pts {
                if let Some((q, _)) = self.cam.project(*p + v3(0.0, 0.0, 0.3)) {
                    painter.circle_filled(q, 5.0, Color32::from_rgb(255, 150, 50));
                }
            }
            return;
        }
        let r = self.terrain.radius;
        let pts: Vec<Pos2> = (0..=48)
            .filter_map(|k| {
                let a = k as f32 / 48.0 * std::f32::consts::TAU;
                let (x, y) = (c.x + r * a.cos(), c.y + r * a.sin());
                let z = t.height_at(x, y).unwrap_or(c.z) + 0.2;
                self.cam.project(v3(x, y, z)).map(|q| q.0)
            })
            .collect();
        let col = if self.terrain.brush == Brush::Vegetation { Color32::from_rgb(90, 220, 90) } else { Color32::from_rgb(255, 200, 60) };
        painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0_f32, col)));
    }

    /// Onglet « Terrain » : parties Créer, Sculpter, Peindre, Routes, Végétation, Exporter.
    pub fn terrain_tab(&mut self, ui: &mut egui::Ui) {
        const SECTIONS: [(&str, &str); 8] = [
            ("Créer", "Nouveau terrain, modèles de départ, carte vide"),
            ("Sculpter", "Monter, creuser, aplanir, lisser, relief naturel (F2–F6)"),
            ("Peindre", "Sable, terre, herbe, roche, béton, goudron (F7)"),
            ("Routes", "Rues, avenues, autoroutes, routes de campagne, chemins (F8)"),
            ("Végétation", "Arbres, palmiers, buissons, rochers du jeu au pinceau (F9)"),
            ("Eau", "Lacs, bassins, rivières : glisser un rectangle (F10)"),
            ("Effets", "Feu, fumée, vapeur, fontaines, feuilles, mouches… du jeu (F11)"),
            ("Exporter", "Pour FiveM ou pour Blender / Sollumz"),
        ];
        ui.horizontal_wrapped(|ui| {
            for (i, (n, tip)) in SECTIONS.iter().enumerate() {
                if ui.selectable_label(self.terrain.section == i as u8, RichText::new(*n).strong()).on_hover_text(*tip).clicked() {
                    self.terrain_section(i as u8);
                }
            }
        });
        ui.separator();
        if self.proj.terrain.is_none() && self.terrain.section != 0 {
            ui.label("Crée d'abord un terrain (partie « Créer »).");
            if ui.button("Aller à « Créer »").clicked() {
                self.terrain.section = 0;
            }
            return;
        }
        match self.terrain.section {
            0 => self.terrain_ui_create(ui),
            1 => self.terrain_ui_sculpt(ui),
            2 => self.terrain_ui_paint(ui),
            3 => self.terrain_ui_roads(ui),
            4 => self.terrain_ui_vegetation(ui),
            5 => self.terrain_ui_water(ui),
            6 => self.terrain_ui_fx(ui),
            _ => self.terrain_ui_export(ui),
        }
        if let Some(t) = &self.proj.terrain {
            ui.separator();
            ui.label(RichText::new(format!("Terrain {:.0} × {:.0} m, cases de {:.0} m · {} route(s) · {} objet(s) de végétation", (t.nx - 1) as f32 * t.cell, (t.ny - 1) as f32 * t.cell, t.cell, t.roads.len(), self.veg_count())).small().weak());
        }
    }

    /// Change de partie et prend l'outil qui va avec.
    pub fn terrain_section(&mut self, s: u8) {
        self.terrain.section = s;
        let b = match s {
            1 => Some(if matches!(self.terrain.brush, Brush::Raise | Brush::Lower | Brush::Flatten | Brush::Smooth | Brush::Noise) { self.terrain.brush } else { Brush::Raise }),
            2 => Some(Brush::Paint),
            3 => Some(Brush::Road),
            4 => Some(Brush::Vegetation),
            5 => Some(Brush::Water),
            6 => Some(Brush::Particles),
            _ => None,
        };
        if let Some(b) = b {
            self.set_terrain_brush(b);
        }
    }

    /// Choisit un pinceau (et l'outil Terrain).
    pub fn set_terrain_brush(&mut self, b: Brush) {
        self.terrain.brush = b;
        self.terrain.section = match b {
            Brush::Paint => 2,
            Brush::Road => 3,
            Brush::Vegetation => 4,
            Brush::Water => 5,
            Brush::Particles => 6,
            _ => 1,
        };
        self.tool = crate::app::Tool::Terrain;
        self.terrain_open = true;
        if b != Brush::Road {
            self.terrain.road_pts.clear();
        }
    }

    fn terrain_ui_create(&mut self, ui: &mut egui::Ui) {
        if ui.checkbox(&mut self.game.empty_map, "Carte vide (cacher toute la carte du jeu)").changed() {
            self.status = if self.game.empty_map { "Carte du jeu cachée : tu pars de zéro (la mer reste).".into() } else { "Carte du jeu réaffichée.".into() };
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Modèle de départ").strong());
        egui::Grid::new("presets").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, (n, tip, style)) in PRESETS.iter().enumerate() {
                let col = if *style == 1 { Color32::from_rgb(214, 190, 140) } else { Color32::from_rgb(120, 170, 100) };
                let txt = RichText::new(*n).color(if self.terrain.preset == i as u8 { Color32::BLACK } else { col });
                let b = egui::Button::new(txt).min_size(egui::vec2(96.0, 26.0)).fill(if self.terrain.preset == i as u8 { col } else { Color32::from_gray(40) });
                if ui.add(b).on_hover_text(*tip).clicked() {
                    self.terrain.preset = i as u8;
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.label(RichText::new(PRESETS[self.terrain.preset as usize].1).small().weak());
        egui::Grid::new("newterr").num_columns(2).show(ui, |ui| {
            ui.label("Taille");
            ui.add(egui::Slider::new(&mut self.terrain.new_size, 128.0..=1600.0).suffix(" m"));
            ui.end_row();
            ui.label("Hauteur de base");
            ui.add(egui::Slider::new(&mut self.terrain.new_height, -20.0..=60.0).suffix(" m"));
            ui.end_row();
            ui.label("Variante");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.terrain.seed).range(1..=99999));
                if ui.small_button("🎲").on_hover_text("Autre variante au hasard").clicked() {
                    self.terrain.seed = 1 + self.rng.next_u32() % 99999;
                }
            });
            ui.end_row();
            if self.terrain.preset == 0 {
                ui.label("Sol");
                egui::ComboBox::from_id_salt("newmat").selected_text(MATERIALS[self.terrain.new_mat as usize].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in MATERIALS.iter().enumerate().take(4) {
                        ui.selectable_value(&mut self.terrain.new_mat, i as u8, *n);
                    }
                });
                ui.end_row();
            }
        });
        let label = if self.proj.terrain.is_none() { "Créer le terrain au centre de la vue" } else { "Remplacer le terrain par ce modèle" };
        if ui.button(RichText::new(label).strong()).clicked() {
            self.push_undo();
            let c = self.cam.target;
            let mut t = Terrain::new(c, self.terrain.new_size, self.terrain.new_height, self.terrain.new_mat);
            if self.terrain.preset != 0 {
                t.generate(self.terrain.preset, self.terrain.seed, self.terrain.new_height);
            }
            self.proj.terrain = Some(t);
            self.terrain.dirty = true;
            self.game.empty_map = true;
            self.terrain_section(1);
            self.status = format!("Terrain « {} » créé : sculpte, peins, trace des routes et sème de la végétation.", PRESETS[self.terrain.preset as usize].0);
        }
        if self.proj.terrain.is_some() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui.button("Peindre selon le relief").on_hover_text("Plages au bord de l'eau, roche sur les pentes, herbe ou sable ailleurs").clicked() {
                    self.push_undo();
                    let style = PRESETS[self.terrain.preset as usize].2;
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.auto_paint(style);
                    }
                    self.terrain.dirty = true;
                }
                if ui.button("Tout aplanir").clicked() {
                    self.push_undo();
                    let h0 = self.terrain.new_height;
                    if let Some(t) = self.proj.terrain.as_mut() {
                        std::sync::Arc::make_mut(&mut t.h).iter_mut().for_each(|v| *v = h0);
                        t.roads.clear();
                    }
                    self.terrain.dirty = true;
                }
                if ui.button("Supprimer le terrain").clicked() {
                    self.push_undo();
                    self.proj.terrain = None;
                }
            });
        }
    }

    fn brush_sliders(&mut self, ui: &mut egui::Ui, strength: bool) {
        egui::Grid::new("brush").num_columns(2).show(ui, |ui| {
            ui.label("Rayon");
            ui.add(egui::Slider::new(&mut self.terrain.radius, 2.0..=120.0).suffix(" m")).on_hover_text("+ / - au clavier");
            ui.end_row();
            if strength {
                ui.label("Force");
                ui.add(egui::Slider::new(&mut self.terrain.strength, 0.5..=20.0));
                ui.end_row();
            }
        });
    }

    fn terrain_ui_sculpt(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (b, n, tip) in [
                (Brush::Raise, "Monter", "Fait monter le sol sous le pinceau (F2)"),
                (Brush::Lower, "Creuser", "Fait descendre le sol (F3)"),
                (Brush::Flatten, "Pelleteuse", "Aplanit à la hauteur du premier point touché (F4)"),
                (Brush::Smooth, "Lisser", "Adoucit les bosses (F5)"),
                (Brush::Noise, "Relief naturel", "Ajoute des bosses irrégulières, comme un vrai sol (F6)"),
            ] {
                if ui.selectable_label(self.terrain.brush == b, n).on_hover_text(tip).clicked() {
                    self.set_terrain_brush(b);
                }
            }
        });
        self.brush_sliders(ui, true);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Île au centre de la vue").on_hover_text("Colline de sable et d'herbe, plage en pente douce").clicked() {
                self.push_undo();
                let c = self.cam.target;
                if let Some(t) = self.proj.terrain.as_mut() {
                    t.island(c, 90.0, 18.0);
                }
                self.terrain.dirty = true;
            }
        });
        ui.label(RichText::new("Glisser dans la vue pour sculpter · Ctrl+Z annule un coup de pinceau").small().weak());
    }

    fn terrain_ui_paint(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (i, (n, c)) in MATERIALS.iter().enumerate() {
                let txt = RichText::new(*n).color(if c[0] as u32 + c[1] as u32 + c[2] as u32 > 380 { Color32::BLACK } else { Color32::WHITE }).background_color(Color32::from_rgb(c[0], c[1], c[2]));
                if ui.selectable_label(self.terrain.paint == i as u8, txt).clicked() {
                    self.terrain.paint = i as u8;
                    self.set_terrain_brush(Brush::Paint);
                }
            }
        });
        self.brush_sliders(ui, false);
        self.game_textures_toggle(ui);
        if ui.button("Peindre tout selon le relief").clicked() {
            self.push_undo();
            let style = PRESETS[self.terrain.preset as usize].2;
            if let Some(t) = self.proj.terrain.as_mut() {
                t.auto_paint(style);
            }
            self.terrain.dirty = true;
        }
    }

    /// Case « Vraies textures du jeu » (terrain et routes).
    pub fn game_textures_toggle(&mut self, ui: &mut egui::Ui) {
        let mut on = crate::groundtex::enabled();
        let tip = "Sable, terre, herbe, roche, béton, goudron et trottoirs avec les textures de GTA V (dictionnaires globalroads, globaldesertterrain, globalcanyonterrain). À l'export, rien n'est copié : le jeu les charge lui-même. Les marquages et passages piétons restent ceux de DragoWalker.";
        if ui.checkbox(&mut on, "Vraies textures du jeu").on_hover_text(tip).changed() {
            crate::groundtex::set_enabled(on);
            self.terrain.dirty = true;
        }
        if on && !crate::groundtex::loaded() {
            ui.label(RichText::new("Charge le jeu (onglet Jeu) pour les voir dans l'éditeur ; l'export les utilise déjà.").small().weak());
        }
    }

    fn terrain_ui_roads(&mut self, ui: &mut egui::Ui) {
        // galerie : chaque type de route vu de dessus
        for (i, k) in crate::roads::KINDS.iter().enumerate() {
            let on = self.terrain.road_kind == i as u8;
            let tex = self
                .road_thumbs
                .entry(i as u8)
                .or_insert_with(|| ui.ctx().load_texture(format!("dw_road_preview_{i}"), crate::roads::preview_image(i), egui::TextureOptions::LINEAR))
                .clone();
            let fill = if on { Color32::from_rgb(70, 52, 30) } else { Color32::from_rgb(30, 33, 39) };
            let r = egui::Frame::new().fill(fill).corner_radius(6.0).inner_margin(4.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(160.0, 56.0)).corner_radius(4.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(k.name).strong().color(if on { Color32::from_rgb(255, 190, 90) } else { Color32::from_gray(225) }));
                        ui.label(RichText::new(format!("{:.0} m de large", k.width + 2.0 * k.sidewalk.max(k.shoulder))).small());
                        ui.label(RichText::new(k.tip).small().weak());
                    });
                });
            });
            if ui.interact(r.response.rect, egui::Id::new(("road_card", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.terrain.road_kind = i as u8;
                self.set_terrain_brush(Brush::Road);
            }
        }
        ui.label(RichText::new("Relier : un point posé près du bout d'une route s'y accroche (cercle bleu) ; sur une route, il fait un carrefour (cercle vert). Une route qui en croise une autre fait aussi un carrefour.").small());
        ui.add_space(4.0);
        ui.label(RichText::new("Clic : poser un point · double-clic ou Entrée : construire · Retour arrière : enlever le dernier point · Échap : annuler").small());
        let n = self.terrain.road_pts.len();
        ui.horizontal(|ui| {
            if ui.add_enabled(n >= 2, egui::Button::new(format!("Construire la route ({n} points)"))).clicked() {
                self.finish_road();
            }
            if ui.add_enabled(n > 0, egui::Button::new("Annuler")).clicked() {
                self.terrain.road_pts.clear();
            }
        });
        let Some(t) = self.proj.terrain.as_ref() else { return };
        if !t.roads.is_empty() {
            ui.separator();
            ui.label(RichText::new("Routes du terrain").strong());
            let mut del: Option<usize> = None;
            let mut goto: Option<V3> = None;
            let mut traffic_edit: Option<(usize, crate::roads::RoadTraffic)> = None;
            for (i, r) in t.roads.iter().enumerate() {
                egui::CollapsingHeader::new(RichText::new("Circulation").small()).id_salt(("road_traffic", i)).show(ui, |ui| {
                    let mut tr = r.traffic.clone();
                    let mut ch = ui.checkbox(&mut tr.enabled, "Voitures sur cette route").changed();
                    ch |= ui.add(egui::Slider::new(&mut tr.density, 0..=15).text("densité")).changed();
                    egui::ComboBox::from_id_salt(("road_speed", i)).selected_text(["Lente", "Normale", "Rapide", "Très rapide"][tr.speed.min(3) as usize]).show_ui(ui, |ui| {
                        for (k, n) in ["Lente", "Normale", "Rapide", "Très rapide"].iter().enumerate() {
                            ch |= ui.selectable_value(&mut tr.speed, k as u8, *n).changed();
                        }
                    });
                    ch |= ui.checkbox(&mut tr.no_big, "Pas de gros véhicules").changed();
                    if ch {
                        traffic_edit = Some((i, tr));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {:.0} m", r.kind().name, r.length()));
                    if ui.small_button("Aller voir").clicked() {
                        let m = r.samples[r.samples.len() / 2];
                        goto = Some(v3(m[0], m[1], m[2]));
                    }
                    if ui.small_button("Supprimer").on_hover_text("Retire la route (le terrain reste aplani)").clicked() {
                        del = Some(i);
                    }
                });
            }
            if let Some((i, tr)) = traffic_edit {
                if let Some(t) = self.proj.terrain.as_mut() {
                    t.roads[i].traffic = tr;
                }
            }
            if let Some(i) = del {
                self.push_undo();
                let mut gone: std::collections::HashSet<u64> = std::collections::HashSet::new();
                if let Some(t) = self.proj.terrain.as_mut() {
                    let r = t.roads.remove(i);
                    t.junctions.retain(|j| {
                        let keep = r.nearest(j.pos[0], j.pos[1]).map(|(d, _)| d > 1.0).unwrap_or(true);
                        if !keep {
                            gone.extend(j.props.iter().copied());
                        }
                        keep
                    });
                }
                // les feux et panneaux de ses carrefours partent avec elle
                if !gone.is_empty() {
                    self.proj.delete_ids(&gone);
                    self.clean_selection();
                    self.sync_game_project();
                }
                self.terrain.dirty = true;
            }
            if let Some(p) = goto {
                self.cam.target = p;
                self.cam.dist = self.cam.dist.min(120.0);
            }
        }
        self.terrain_ui_signals(ui);
        ui.separator();
        if ui.button("Morceaux de route du jeu…").on_hover_text("Bibliothèque des chaussées, virages et carrefours modelés par Rockstar, à poser bout à bout").clicked() {
            self.buildings.open = true;
            self.buildings.roads = true;
            self.buildings.chosen = None;
        }
    }

    /// Feux rouges et stops des carrefours : automatiques, ou choisis carrefour par carrefour.
    fn terrain_ui_signals(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        if t.junctions.is_empty() {
            return;
        }
        ui.separator();
        ui.label(RichText::new("Feux et stops").strong());
        let mut auto = t.auto_signals;
        let mut changed = false;
        if ui.checkbox(&mut auto, "Poser les feux et les stops aux carrefours").on_hover_text("Vrais objets du jeu : feux à bras (prop_traffic_01a / 01b) après le carrefour, bras au-dessus des voies, comme dans GTA V ; panneaux stop (prop_sign_road_01a) avant le carrefour. Les points de route disent aux voitures de s'arrêter au feu ou au stop. Chaque feu ou panneau reste un objet normal : tu peux le déplacer, le supprimer ou en ajouter d'autres depuis la bibliothèque de props.").changed() {
            changed = true;
        }
        let (l, s) = self.signals_count();
        ui.label(RichText::new(format!("{l} feu(x), {s} stop(s) posés · automatique : feux entre routes de même taille en ville, stops sur la plus petite route quand une rue rejoint une avenue, stops partout à la campagne")).small().weak());
        let mut set: Option<(usize, u8)> = None;
        let mut goto: Option<V3> = None;
        for (i, j) in t.junctions.iter().enumerate() {
            ui.horizontal(|ui| {
                let ctl = crate::signals::arm_controls(j);
                let what = match (ctl.iter().filter(|c| **c == 1).count(), ctl.iter().filter(|c| **c == 2).count()) {
                    (0, 0) => "rien".to_string(),
                    (a, 0) => format!("{a} feux"),
                    (0, b) => format!("{b} stops"),
                    (a, b) => format!("{a} feux, {b} stops"),
                };
                ui.label(format!("Carrefour {} ({} branches) : {what}", i + 1, j.arms.len()));
                let mut c = j.control.min(3);
                egui::ComboBox::from_id_salt(("junction_ctl", i)).width(110.0).selected_text(crate::signals::CONTROL_NAMES[c as usize]).show_ui(ui, |ui| {
                    for (k, n) in crate::signals::CONTROL_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut c, k as u8, *n).changed() {
                            set = Some((i, c));
                        }
                    }
                });
                if ui.small_button("Aller voir").clicked() {
                    goto = Some(v3(j.pos[0], j.pos[1], j.pos[2]));
                }
            });
        }
        if changed || set.is_some() {
            self.push_undo();
            if let Some(t) = self.proj.terrain.as_mut() {
                t.auto_signals = auto;
                if let Some((i, c)) = set {
                    t.junctions[i].control = c;
                }
            }
            self.refresh_signals();
            let (l, s) = self.signals_count();
            self.status = format!("Signalisation des carrefours : {l} feu(x), {s} stop(s).");
        }
        if let Some(p) = goto {
            self.cam.target = p;
            self.cam.dist = self.cam.dist.min(60.0);
        }
    }

    fn terrain_ui_vegetation(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("vegsets").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, s) in crate::vegetation::SETS.iter().enumerate() {
                let n = self.veg_props(i).len();
                let b = egui::Button::new(RichText::new(s.name).color(if self.terrain.veg_set == i { Color32::BLACK } else { Color32::from_rgb(140, 210, 120) }))
                    .min_size(egui::vec2(140.0, 26.0))
                    .fill(if self.terrain.veg_set == i { Color32::from_rgb(140, 210, 120) } else { Color32::from_gray(40) });
                if ui.add(b).on_hover_text(format!("{} · {n} modèles du jeu · écart {:.0} m", s.tip, s.spacing)).clicked() {
                    self.terrain.veg_set = i;
                    self.terrain.veg_mode = 0;
                    self.set_terrain_brush(Brush::Vegetation);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        egui::Grid::new("vegbrush").num_columns(2).show(ui, |ui| {
            ui.label("Rayon");
            ui.add(egui::Slider::new(&mut self.terrain.radius, 2.0..=120.0).suffix(" m"));
            ui.end_row();
            ui.label("Densité");
            ui.add(egui::Slider::new(&mut self.terrain.veg_density, 0.05..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
            ui.end_row();
        });
        let s = &crate::vegetation::SETS[self.terrain.veg_set];
        ui.label(RichText::new(format!("Modèles : {}", self.veg_props(self.terrain.veg_set).join(", "))).small().weak());
        ui.label(RichText::new(format!("Glisser : semer ({}) · Ctrl + glisser : retirer · pas sur les routes, le béton ni l'eau", s.name)).small());
        ui.horizontal(|ui| {
            if ui.button("Retirer toute la végétation").clicked() {
                self.push_undo();
                let n = self.veg_remove(V3::ZERO, 1.0e7);
                self.status = format!("{n} objet(s) de végétation retiré(s).");
            }
        });
        // « Herbe du jeu » et « Biomes » (grass.rs) : retirés de l'interface le 11 nov. 2026 à la demande de la
        // personne (pas encore vus fonctionner en jeu) — à reprendre plus tard, voir A_FAIRE.md
    }

    fn terrain_ui_export(&mut self, ui: &mut egui::Ui) {
        ui.label("Pour FiveM : menu Fichier » Exporter une ressource FiveM. Le terrain part en tuiles (modèle + collision), les routes avec leurs marquages et leur collision, la végétation dans la carte de destination.");
        ui.add_space(4.0);
        if ui.button("Exporter pour Sollumz / Blender").on_hover_text("Écrit le terrain et les routes comme modèle (.ydr.xml + textures) dans le dossier d'échange Sollumz").clicked() {
            if let Some(t) = self.proj.terrain.as_ref() {
                let (m, k, tx) = t.model();
                let dir = std::path::PathBuf::from(&self.sollumz.dir);
                self.status = match crate::sollumz::write_ydr_xml(&dir, "dw_terrain", &m, &k, &tx) {
                    Ok(p) => format!("Terrain exporté : {} (origine {:.1} {:.1}). Ouvre-le dans Blender avec Sollumz.", p.display(), t.origin.x, t.origin.y),
                    Err(e) => format!("Export impossible : {e}"),
                };
            }
        }
    }
}

/// Matériaux de collision du jeu (ordre du fichier materials.dat) pour chaque matériau du terrain.
const COLL_NAMES: [&str; 7] = ["SAND_COMPACT", "DIRT_TRACK", "GRASS", "ROCK", "CONCRETE", "TARMAC", "TARMAC"];

/// Index des matériaux de collision : lus dans materials.dat du jeu, sinon l'ordre habituel — **estimé**.
fn collision_types(w: Option<&crate::gta::world::GameWorld>) -> [u32; 7] {
    let mut out = [19, 35, 47, 9, 1, 4, 4];
    let names = material_names(w);
    for (i, n) in COLL_NAMES.iter().enumerate() {
        if let Some(k) = names.iter().position(|x| x == n) {
            out[i] = k as u32;
        }
    }
    out
}

/// Index d'un type de surface de collision par son nom (materials.dat), sinon `fallback`.
fn coll_type(names: &[String], name: &str, fallback: u32) -> u32 {
    names.iter().position(|x| x == name).map(|k| k as u32).unwrap_or(fallback)
}

/// Noms des types de surface, dans l'ordre de materials.dat du jeu (vide sans le jeu).
pub fn material_names(w: Option<&crate::gta::world::GameWorld>) -> Vec<String> {
    let Some(w) = w else { return Vec::new() };
    let Some(f) = w.vfs.files.iter().find(|f| f.name.eq_ignore_ascii_case("materials.dat")) else { return Vec::new() };
    let Some(d) = w.vfs.read_binary(f, &w.keys) else { return Vec::new() };
    let txt = String::from_utf8_lossy(&d);
    txt
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
        .filter(|n| n.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) && n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false))
        .collect()
}

/// Type d'objet (CBaseArchetypeDef) d'un morceau du terrain exporté : boîte dans le repère du morceau,
/// dictionnaire de textures `txd` (celui de DragoWalker ou un dictionnaire du jeu).
fn arch_xml(name: &str, txd: &str, lmn: V3, lmx: V3, lod: f32) -> String {
    let c = (lmn + lmx) * 0.5;
    let r = (lmx - lmn).len() * 0.5;
    format!(
        "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{lod}\" />\n   <flags value=\"{TERRAIN_ARCH_FLAGS}\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"200\" />\n   <name>{name}</name>\n   <textureDictionary>{txd}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{name}</assetName>\n   <extensions />\n  </Item>",
        lmn.x, lmn.y, lmn.z, lmx.x, lmx.y, lmx.z, c.x, c.y, c.z
    )
}

/// Distance d'affichage d'une tuile détaillée quand elle a une version de loin (m) — **estimé** ; au-delà, la
/// version simplifiée (TERRAIN_LOD_STEP fois moins de sommets par côté) prend le relais jusqu'à TERRAIN_LOD_DIST.
pub const TERRAIN_HD_DIST: f32 = 500.0;
pub const TERRAIN_LOD_STEP: usize = 4;

/// Distance de la tuile détaillée comptée depuis son coin (position du placement, d'où CodeWalker mesure :
/// RenderLodManager.GetEntityChildren 0x060009a8, `Distance = |Position − caméra|`) : TERRAIN_HD_DIST plus la
/// demi-diagonale, pour que tout point à TERRAIN_HD_DIST du centre de la tuile la voie détaillée.
pub fn hd_dist_from_corner(w: f32, h: f32) -> f32 {
    TERRAIN_HD_DIST + 0.5 * (w * w + h * h).sqrt()
}

/// Associe chaque morceau détaillé d'une tuile (nom, dictionnaire de textures) à un morceau de sa version de loin :
/// celui du même dictionnaire, sinon le premier. Rend (parent de chaque morceau détaillé, morceaux de loin gardés).
/// Un morceau de loin sans enfant serait une racine dessinée jusqu'à sa distance même quand la tuile détaillée
/// l'est (RenderLodManager.Update 0x060009a6 : racine dessinée si Distance ≤ LodDist, enfants seulement via
/// GetEntityChildren 0x060009a8) : il n'est donc pas placé.
pub fn pair_lod_pieces(hd: &[(String, String)], lods: &[(String, String)]) -> (Vec<Option<usize>>, Vec<usize>) {
    let parent_of: Vec<Option<usize>> = hd.iter().map(|(_, t)| if lods.is_empty() { None } else { Some(lods.iter().position(|(_, lt)| lt == t).unwrap_or(0)) }).collect();
    let kept: Vec<usize> = (0..lods.len()).filter(|li| parent_of.contains(&Some(*li))).collect();
    (parent_of, kept)
}

/// Placement d'un morceau du terrain avec niveau de détail : `lod_level` (LODTYPES_…), `parent` (index dans la
/// carte parente, -1 aucun), drapeaux (bit 0x8 si le parent est dans la carte parente, comme les objets du jeu),
/// nombre d'enfants et distance de leurs enfants.
#[allow(clippy::too_many_arguments)]
fn ent_xml_lod(name: &str, guid: u32, x: f32, y: f32, lod: f32, lod_level: &str, parent: i32, flags: u32, nchild: u32, child_lod: f32) -> String {
    format!(
        "  <Item type=\"CEntityDef\">\n   <archetypeName>{name}</archetypeName>\n   <flags value=\"{flags}\" />\n   <guid value=\"{guid}\" />\n   <position x=\"{x}\" y=\"{y}\" z=\"0\" />\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"{parent}\" />\n   <lodDist value=\"{lod}\" />\n   <childLodDist value=\"{child_lod}\" />\n   <lodLevel>{lod_level}</lodLevel>\n   <numChildren value=\"{nchild}\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n"
    )
}

/// Placement (CEntityDef) d'un morceau du terrain exporté, au point (x, y, 0).
fn ent_xml(name: &str, guid: u32, x: f32, y: f32, lod: f32) -> String {
    format!(
        "  <Item type=\"CEntityDef\">\n   <archetypeName>{name}</archetypeName>\n   <flags value=\"{TERRAIN_ENTITY_FLAGS}\" />\n   <guid value=\"{guid}\" />\n   <position x=\"{x}\" y=\"{y}\" z=\"0\" />\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"{lod}\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>LODTYPES_DEPTH_HD</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n"
    )
}

/// Écrit un modèle exporté, coupé selon le dictionnaire de ses textures : les parties à textures DragoWalker
/// gardent le nom `name` et le dictionnaire `own_txd` ; celles qui utilisent une vraie texture du jeu vont dans
/// `name_gN` (N = dictionnaire du jeu dans groundtex::TXDS), dont les matériaux nomment la texture du jeu.
/// Un type d'objet n'a qu'un dictionnaire de textures : d'où un modèle par dictionnaire, posé au même endroit.
/// Rend (nom du modèle, dictionnaire) de chaque modèle écrit.
fn write_split(stream: &std::path::Path, name: &str, m: &RModel, keys: &[u64], texs: &[(u64, TexData)], own_txd: &str) -> Result<Vec<(String, String)>, String> {
    let group = |k: u64| crate::groundtex::game_ref(k).and_then(|g| crate::groundtex::TXDS.iter().position(|t| *t == g.txd));
    let mut out = Vec::new();
    let mut groups: Vec<Option<usize>> = keys.iter().map(|k| group(*k)).collect();
    groups.sort();
    groups.dedup();
    for g in groups {
        let sel: Vec<usize> = (0..m.geoms.len()).filter(|&i| group(keys.get(i).copied().unwrap_or(0)) == g).collect();
        if sel.is_empty() {
            continue;
        }
        let sub = RModel { verts: m.verts.clone(), idx: m.idx.clone(), geoms: sel.iter().map(|&i| m.geoms[i].clone()).collect(), embedded_dict: None };
        let sub_keys: Vec<u64> = sel.iter().map(|&i| keys[i]).collect();
        match g {
            None => {
                crate::sollumz::write_ydr_xml_ex(stream, name, &sub, &sub_keys, texs, Some(own_txd))?;
                out.push((name.to_string(), own_txd.to_string()));
            }
            Some(gi) => {
                let pn = format!("{name}_g{gi}");
                crate::sollumz::write_ydr_xml_named(stream, &pn, &sub, &sub_keys, &|k| crate::groundtex::game_ref(k).map(|t| t.tex.to_string()))?;
                out.push((pn, crate::groundtex::TXDS[gi].to_string()));
            }
        }
    }
    Ok(out)
}

fn xyz(tag: &str, v: V3) -> String {
    format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", v.x, v.y, v.z)
}

impl Terrain {
    /// Collisions du terrain au format XML de CodeWalker (BoundsFile > Composite > GeometryBVH), dans le
    /// monde ; blocs de 100 × 100 cases (indices des sommets sur 16 bits). Format relevé sur un .ybn du jeu
    /// converti par YbnXml.GetXml ; reconverti en binaire par CodeWalker (XmlMeta.GetData, format Ybn).
    /// Les voisins des triangles (f1–f3), la quantification, les matériaux utilisés, l'arbre de recherche et les
    /// drapeaux sont recalculés par CodeWalker à la conversion (BoundGeometry::ReadXml 0x0600410a,
    /// BoundComposite::ReadXml 0x06004170) : on les laisse à 0 / par défaut ici.
    /// `pids` : ProceduralID de chaque herbe (pids[0] = 0 sans herbe, pids[k] = herbe k) ; un matériau par couple
    /// (surface, herbe), triangle m = herbe × 7 + surface.
    pub fn collision_xml(&self, types: [u32; 7], pids: &[u32]) -> String {
        use std::fmt::Write;
        let (mn, mx) = self.bounds();
        let c = (mn + mx) * 0.5;
        let r = (mx - mn).len() * 0.5;
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n <Bounds type=\"Composite\">\n");
        let head = |s: &mut String, ind: &str, a: V3, b: V3, margin: f32| {
            let c = (a + b) * 0.5;
            let _ = writeln!(s, "{ind}{}\n{ind}{}\n{ind}{}\n{ind}{}\n{ind}<SphereRadius value=\"{}\" />\n{ind}<Margin value=\"{margin}\" />\n{ind}<Volume value=\"1\" />\n{ind}<Inertia x=\"1\" y=\"1\" z=\"1\" />\n{ind}<MaterialIndex value=\"0\" />\n{ind}<MaterialColourIndex value=\"0\" />\n{ind}<ProceduralID value=\"0\" />\n{ind}<RoomID value=\"0\" />\n{ind}<PedDensity value=\"0\" />\n{ind}<UnkFlags value=\"0\" />\n{ind}<PolyFlags value=\"0\" />\n{ind}<UnkType value=\"1\" />", xyz("BoxMin", a), xyz("BoxMax", b), xyz("BoxCenter", c), xyz("SphereCenter", c), (b - a).len() * 0.5);
        };
        head(&mut s, "  ", mn, mx, 0.0);
        let _ = r;
        let _ = c;
        s.push_str("  <Children>\n");
        let block = 100usize;
        let mut j0 = 0;
        while j0 < self.ny - 1 {
            let j1 = (j0 + block).min(self.ny - 1);
            let mut i0 = 0;
            while i0 < self.nx - 1 {
                let i1 = (i0 + block).min(self.nx - 1);
                // sommets du bloc
                let (mut bmn, mut bmx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
                let mut verts = String::new();
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        let p = self.vpos(i, j);
                        bmn = bmn.min(p);
                        bmx = bmx.max(p);
                        let _ = writeln!(verts, "     {}, {}, {}", p.x, p.y, p.z);
                    }
                }
                let w = i1 - i0 + 1;
                let mut used = [false; 7];
                let mut polys = String::new();
                // les sommets sont relatifs au centre de la pièce (GeometryCenter) : c'est ainsi que les lit
                // CodeWalker (les .ybn.xml de Sollumz de la map désert, qui marchent en jeu, font pareil) ;
                // en coordonnées monde, la collision se retrouvait deux fois plus loin, hors de la map
                let gc = (bmn + bmx) * 0.5;
                let mut rel = String::with_capacity(verts.len());
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        let p = self.vpos(i, j) - gc;
                        let _ = writeln!(rel, "     {}, {}, {}", p.x, p.y, p.z);
                    }
                }
                let verts = rel;
                for j in j0..j1 {
                    for i in i0..i1 {
                        let g = (self.grass_at(i, j) as usize).min(pids.len().saturating_sub(1));
                        let m = (self.mat[j * (self.nx - 1) + i] as usize).min(6) + g * 7;
                        used[m % 7] = true;
                        let a = (j - j0) * w + (i - i0);
                        let (b, cc, d) = (a + 1, a + w, a + w + 1);
                        let _ = writeln!(polys, "     <Triangle m=\"{m}\" v1=\"{a}\" v2=\"{b}\" v3=\"{d}\" f1=\"0\" f2=\"0\" f3=\"0\" />");
                        let _ = writeln!(polys, "     <Triangle m=\"{m}\" v1=\"{a}\" v2=\"{d}\" v3=\"{cc}\" f1=\"0\" f2=\"0\" f3=\"0\" />");
                    }
                }
                let _ = used;
                s.push_str("   <Item type=\"GeometryBVH\">\n");
                head(&mut s, "    ", bmn, bmx, 0.04);
                let gc = (bmn + bmx) * 0.5;
                let _ = writeln!(s, "    <CompositeTransform>\n     1 0 0 0\n     0 1 0 0\n     0 0 1 0\n     0 0 0 1\n    </CompositeTransform>\n    <CompositeFlags1>MAP_WEAPON, MAP_DYNAMIC, MAP_ANIMAL, MAP_COVER, MAP_VEHICLE</CompositeFlags1>\n    <CompositeFlags2>VEHICLE_NOT_BVH, VEHICLE_BVH, PED, RAGDOLL, ANIMAL, ANIMAL_RAGDOLL, OBJECT, PLANT, PROJECTILE, EXPLOSION, FORKLIFT_FORKS, TEST_WEAPON, TEST_CAMERA, TEST_AI, TEST_SCRIPT, TEST_VEHICLE_WHEEL, GLASS</CompositeFlags2>\n    {}\n    <UnkFloat1 value=\"0\" />\n    <UnkFloat2 value=\"0.0025\" />\n    <Materials>", xyz("GeometryCenter", gc));
                for pid in pids.iter().copied().chain(if pids.is_empty() { Some(0) } else { None }) {
                    for t in types {
                        let _ = writeln!(s, "     <Item>\n      <Type value=\"{t}\" />\n      <ProceduralID value=\"{pid}\" />\n      <RoomID value=\"0\" />\n      <PedDensity value=\"0\" />\n      <Flags>NONE</Flags>\n      <MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />\n     </Item>");
                    }
                }
                let _ = writeln!(s, "    </Materials>\n    <Vertices>\n{verts}    </Vertices>\n    <Polygons>\n{polys}    </Polygons>\n   </Item>");
                i0 = i1;
            }
            j0 = j1;
        }
        s.push_str("  </Children>\n </Bounds>\n</BoundsFile>\n");
        s
    }
}

impl DwApp {
    /// Exporte le terrain dans le dossier stream : modèle (.ydr), collisions (.ybn), type d'objet (.ytyp) et
    /// placement (.ymap), en XML pour CodeWalker qui les convertit ensuite en fichiers du jeu.
    /// Comme la map désert (E:\DESERT-MAP, validée en jeu) : le terrain est coupé en tuiles de 100 × 100 cases ;
    /// chaque tuile a son modèle (.ydr) et sa collision à part (.ybn, dans le monde, nom différent du modèle) ;
    /// toutes les textures sont dans un seul dictionnaire (.ytd) déclaré dans le type d'objet ; les objets sont
    /// visibles de loin (lodDist 3000, niveau HD sans parent, PRI_REQUIRED, sans hiérarchie de LOD : dans la
    /// map désert, la bascule HD ↔ LOD faisait perdre des collisions ; son « ORPHANHLOD » est relu HD par
    /// CodeWalker, le nom exact étant LODTYPES_DEPTH_ORPHANHD — rage__eLodType).
    pub fn export_terrain(&self, stream: &std::path::Path) -> Result<Vec<String>, String> {
        let Some(t) = self.proj.terrain.as_ref() else { return Ok(Vec::new()) };
        // anciens fichiers du terrain (export précédent, autre découpage) : retirés pour ne pas rester en jeu
        if let Ok(rd) = std::fs::read_dir(stream) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("dw_terrain") && n.ends_with(".xml") && f.path().is_file() {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
        let txd = TERRAIN_TXD;
        let lod = TERRAIN_LOD_DIST;
        let types = collision_types(self.game.world.as_deref());
        // herbe du jeu : ProceduralID de chaque herbe, lu dans procedural.meta (procTagTable)
        let pids = crate::grass::proc_ids(self.game.world.as_deref());
        let mut written_tex = false;
        let mut archs = Vec::new();
        let mut ents = String::new();
        let big = 3.0e38f32;
        let (mut emn, mut emx) = (v3(big, big, big), v3(-big, -big, -big));
        let (mut smn, mut smx) = (emn, emx);
        let mut tiles = 0;
        // versions de loin des tuiles : leurs placements et leur nombre
        let mut lod_ents = String::new();
        let mut lod_n = 0usize;
        // numéros d'objet à part de ceux des effets (export_scenery compte avec `tiles`)
        let mut guid_n = 0x8000u32;
        let block = 100usize;
        let mut j0 = 0;
        let mut row = 0;
        while j0 < t.ny - 1 {
            let j1 = (j0 + block).min(t.ny - 1);
            let mut i0 = 0;
            let mut col = 0;
            while i0 < t.nx - 1 {
                let i1 = (i0 + block).min(t.nx - 1);
                let tile = t.sub(i0, j0, i1, j1);
                let name = format!("dw_terrain_r{row}c{col}");
                let (m, k, tx) = tile.model();
                if !written_tex {
                    // tous les matériaux, même ceux absents de cette tuile
                    let mut all: Vec<(u64, TexData)> = (0..MATERIALS.len()).map(|m| (material_key(m), material_texture(m))).collect();
                    if !t.roads.is_empty() {
                        all.extend(crate::roads::road_textures());
                    }
                    // les vraies textures du jeu ne sont pas copiées : le jeu les a déjà (voir groundtex.rs) ;
                    // le dictionnaire garde au moins une texture pour exister
                    // (ni les anciennes chaussées à marquages peints, qui ne servent plus qu'aux aperçus)
                    let game = crate::groundtex::enabled();
                    all.retain(|(k, _)| crate::groundtex::game_ref(*k).is_none() && (!game || (0..crate::roads::KINDS.len()).all(|i| crate::roads::road_tex_key(i) != *k)));
                    if all.is_empty() {
                        all = crate::roads::road_textures().into_iter().filter(|(k, _)| *k == crate::roads::LINE_WHITE_KEY).collect();
                    }
                    crate::sollumz::write_ytd_xml(stream, txd, &all)?;
                    written_tex = true;
                }
                let pieces = write_split(stream, &name, &m, &k, &tx, txd)?;
                std::fs::write(stream.join(format!("dw_terrain_col_r{row}c{col}.ybn.xml")), tile.collision_xml(types, &pids)).map_err(|e| e.to_string())?;
                // type d'objet : boîte du modèle (repère de la tuile : x, y depuis son coin, z absolu)
                let (mn, mx) = tile.bounds();
                let lmn = v3(0.0, 0.0, mn.z);
                let lmx = v3(mx.x - mn.x, mx.y - mn.y, mx.z);
                tiles += 1;
                // version de loin de la tuile (carte « dw_terrain_lod », parente) ; son premier morceau est le
                // parent des morceaux détaillés (CMapData.parent + parentIndex + bit 0x8, comme lods.rs)
                let lt = tile.downsample(TERRAIN_LOD_STEP);
                let (lm, lk, ltx) = lt.model();
                let lname = format!("dw_terrain_lod_r{row}c{col}");
                let lpieces = write_split(stream, &lname, &lm, &lk, &ltx, txd)?;
                // la distance est mesurée depuis la position du placement (coin de la tuile) :
                // RenderLodManager.Update 0x060009a6 / GetEntityChildren 0x060009a8 ; on ajoute la demi-diagonale
                // pour que tout point à TERRAIN_HD_DIST du centre voie la tuile détaillée
                let hd = hd_dist_from_corner(mx.x - mn.x, mx.y - mn.y);
                // un morceau de loin sans enfant est une racine dessinée jusqu'à sa distance, même quand la tuile
                // détaillée l'est aussi (0x060009a6) : chaque morceau de loin est donc le parent des morceaux
                // détaillés du même dictionnaire de textures (le premier prend ceux qui n'ont pas d'équivalent)
                let (parent_of, kept) = pair_lod_pieces(&pieces, &lpieces);
                for &li in &kept {
                    let (pn, ptxd) = &lpieces[li];
                    archs.push(arch_xml(pn, ptxd, lmn, lmx, lod));
                    guid_n += 1;
                    let nchild = parent_of.iter().filter(|p| **p == Some(li)).count() as u32;
                    lod_ents.push_str(&ent_xml_lod(pn, 0x0D77_0000u32 + guid_n, tile.origin.x, tile.origin.y, lod, "LODTYPES_DEPTH_LOD", -1, TERRAIN_ENTITY_FLAGS, nchild, hd));
                }
                for ((pn, ptxd), p) in pieces.iter().zip(&parent_of) {
                    // index du parent dans la carte de loin = rang parmi les morceaux de loin gardés
                    let parent = p.and_then(|li| kept.iter().position(|k| *k == li)).map(|r| (lod_n + r) as i32).unwrap_or(-1);
                    let (elod, flags) = if parent < 0 { (lod, TERRAIN_ENTITY_FLAGS) } else { (hd, TERRAIN_ENTITY_FLAGS | crate::mech::LOD_IN_PARENT_YMAP) };
                    archs.push(arch_xml(pn, ptxd, lmn, lmx, elod));
                    guid_n += 1;
                    ents.push_str(&ent_xml_lod(pn, 0x0D77_0000u32 + guid_n, tile.origin.x, tile.origin.y, elod, "LODTYPES_DEPTH_HD", parent, flags, 0, 0.0));
                }
                lod_n += kept.len();
                emn = emn.min(mn);
                emx = emx.max(mx);
                // zone de chargement = boîte ± distance des placements (CalcExtents 0x06002cd5) : la tuile
                // détaillée n'a besoin d'être chargée que jusqu'à sa propre distance quand elle a un parent
                let ext = if kept.is_empty() { lod } else { hd };
                smn = smn.min(mn - v3(ext, ext, ext));
                smx = smx.max(mx + v3(ext, ext, ext));
                i0 = i1;
                col += 1;
            }
            j0 = j1;
            row += 1;
        }
        // routes : un modèle et une collision par route, placés à leur premier point
        let names = material_names(self.game.world.as_deref());
        let mut road_n = 0;
        // les carrefours forment une « route » de plus (modèle + collision)
        let mut all_roads: Vec<(String, Option<&crate::roads::Road>)> = t.roads.iter().enumerate().map(|(i, r)| (format!("dw_road_{i}"), Some(r))).collect();
        if !t.junctions.is_empty() {
            all_roads.push(("dw_road_junctions".into(), None));
        }
        for (ri, (name, rr)) in all_roads.iter().enumerate() {
            let (anchor, pos, uv, parts, ctypes) = match rr {
                Some(r) => {
                    if r.samples.len() < 2 {
                        continue;
                    }
                    let k = r.kind();
                    let anchor = v3(r.samples[0][0].floor(), r.samples[0][1].floor(), 0.0);
                    let (pos, uv, parts) = r.mesh(anchor);
                    // collision : chaussée, trottoirs, accotements (types de surface lus dans materials.dat)
                    let shoulder = COLL_NAMES[(k.shoulder_mat as usize).min(COLL_NAMES.len() - 1)];
                    let ctypes = [coll_type(&names, k.coll, if k.dirt { 35 } else { 4 }), coll_type(&names, "PAVING_SLAB", coll_type(&names, "CONCRETE", 1)), coll_type(&names, shoulder, 1)];
                    (anchor, pos, uv, parts, ctypes)
                }
                None => {
                    let anchor = v3(t.junctions[0].pos[0].floor(), t.junctions[0].pos[1].floor(), 0.0);
                    let (mut pos, mut uv, mut parts): (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) = (Vec::new(), Vec::new(), Vec::new());
                    for j in &t.junctions {
                        let (p, u, pa) = crate::roads::junction_mesh(j, anchor);
                        let base = pos.len() as u32;
                        pos.extend(p);
                        uv.extend(u);
                        for (k, idx, m) in pa {
                            parts.push((k, idx.into_iter().map(|i| i + base).collect(), m));
                        }
                    }
                    let tar = coll_type(&names, "TARMAC", 4);
                    (anchor, pos, uv, parts, [tar, tar, tar])
                }
            };
            let texs: Vec<(u64, TexData)> = (0..MATERIALS.len()).map(|m| (material_key(m), material_texture(m))).chain(crate::roads::road_textures()).collect();
            let (mut verts, mut idx, mut geoms, mut keys) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            append_mesh(&mut verts, &mut idx, &mut geoms, &mut keys, &texs, &pos, &uv, &parts);
            let model = RModel { verts, idx, geoms, embedded_dict: None };
            let pieces = write_split(stream, name, &model, &keys, &texs, txd)?;
            let world: Vec<V3> = pos.iter().map(|p| *p + anchor).collect();
            let mut tris = Vec::new();
            for (_, list, m) in &parts {
                // marquages : pas de collision (la chaussée dessous a la sienne)
                if *m == crate::roads::NO_COLLISION {
                    continue;
                }
                for c in list.chunks_exact(3) {
                    tris.push((c[0], c[1], c[2], *m));
                }
            }
            std::fs::write(stream.join(format!("dw_road_col_{ri}.ybn.xml")), crate::roads::mesh_collision_xml(&world, &tris, &ctypes)).map_err(|e| e.to_string())?;
            let (mut lmn, mut lmx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
            for p in &pos {
                lmn = lmn.min(*p);
                lmx = lmx.max(*p);
            }
            tiles += 1;
            for (pn, ptxd) in &pieces {
                archs.push(arch_xml(pn, ptxd, lmn, lmx, lod));
                guid_n += 1;
                ents.push_str(&ent_xml(pn, 0x0D77_0000u32 + guid_n, anchor.x, anchor.y, lod));
            }
            let (wmn, wmx) = (lmn + anchor, lmx + anchor);
            emn = emn.min(wmn);
            emx = emx.max(wmx);
            smn = smn.min(wmn - v3(lod, lod, lod));
            smx = smx.max(wmx + v3(lod, lod, lod));
            road_n += 1;
        }
        let mut fx_n = 0u32;
        let before = tiles;
        let (fx_archs, fx_ents, fx_ext) = self.export_scenery(stream, &mut tiles)?;
        fx_n += tiles - before;
        tiles = before;
        archs.extend(fx_archs);
        ents.push_str(&fx_ents);
        if let Some((a, b)) = fx_ext {
            emn = emn.min(a + v3(150.0, 150.0, 150.0));
            emx = emx.max(b - v3(150.0, 150.0, 150.0));
            smn = smn.min(a);
            smx = smx.max(b);
        }
        let ytyp = crate::model::Ytyp { name: "dw_terrain_types".into(), mlos: Vec::new(), visible: true, raw_archetypes: archs };
        std::fs::write(stream.join("dw_terrain_types.ytyp.xml"), crate::xmlio::write_ytyp(&ytyp)).map_err(|e| e.to_string())?;
        // placement écrit directement (les étendues doivent couvrir les tuiles entières : le calcul général
        // ne connaît pas la taille des modèles du terrain)
        let vx = |n: &str, p: V3| format!(" <{n} x=\"{}\" y=\"{}\" z=\"{}\" />\n", p.x, p.y, p.z);
        let ymap = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>dw_terrain_placement</name>\n <parent>dw_terrain_lod</parent>\n <flags value=\"0\" />\n <contentFlags value=\"1\" />\n{}{}{}{} <entities>\n{ents} </entities>\n <containerLods />\n <boxOccluders />\n <occludeModels />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>dw_terrain_placement</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n</CMapData>\n",
            vx("streamingExtentsMin", smn),
            vx("streamingExtentsMax", smx),
            vx("entitiesExtentsMin", emn),
            vx("entitiesExtentsMax", emx)
        );
        std::fs::write(stream.join("dw_terrain_placement.ymap.xml"), ymap).map_err(|e| e.to_string())?;
        // carte des versions de loin : parente de dw_terrain_placement ; drapeaux d'une carte de LOD
        // (YmapFile::CalcFlags, model.rs calc_flags : flags 2, contentFlags 2) ; chargée jusqu'à TERRAIN_LOD_DIST
        let (lmn, lmx) = (emn - v3(lod, lod, lod), emx + v3(lod, lod, lod));
        let lymap = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>dw_terrain_lod</name>\n <parent />\n <flags value=\"2\" />\n <contentFlags value=\"2\" />\n{}{}{}{} <entities>\n{lod_ents} </entities>\n <containerLods />\n <boxOccluders />\n <occludeModels />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>dw_terrain_lod</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n</CMapData>\n",
            vx("streamingExtentsMin", lmn),
            vx("streamingExtentsMax", lmx),
            vx("entitiesExtentsMin", emn),
            vx("entitiesExtentsMax", emx)
        );
        std::fs::write(stream.join("dw_terrain_lod.ymap.xml"), lymap).map_err(|e| e.to_string())?;
        Ok(vec![
            format!(
                "Terrain exporté : {} tuile(s) ({} × {} m), {road_n} route(s), {} zone(s) d'eau, {fx_n} effet(s) ; modèles et collisions, textures dans {txd}.ytd, visible jusqu'à {lod} m.",
                tiles - road_n,
                (t.nx - 1) as f32 * t.cell,
                (t.ny - 1) as f32 * t.cell,
                t.waters.len()
            ),
            "dw_terrain_types".to_string(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: &str, t: &str) -> (String, String) {
        (n.to_string(), t.to_string())
    }

    #[test]
    fn lod_pieces_each_have_children() {
        // même dictionnaire → même parent ; un morceau de loin sans enfant n'est pas placé
        let hd = vec![p("a", "dw"), p("a_g1", "g1")];
        let lods = vec![p("l", "dw"), p("l_g1", "g1"), p("l_g2", "g2")];
        let (parent, kept) = pair_lod_pieces(&hd, &lods);
        assert_eq!(parent, vec![Some(0), Some(1)]);
        assert_eq!(kept, vec![0, 1]);
        // morceau détaillé sans équivalent de loin → le premier
        let (parent, kept) = pair_lod_pieces(&[p("a_g3", "g3")], &lods);
        assert_eq!(parent, vec![Some(0)]);
        assert_eq!(kept, vec![0]);
        // pas de version de loin → pas de parent
        let (parent, kept) = pair_lod_pieces(&hd, &[]);
        assert_eq!(parent, vec![None, None]);
        assert!(kept.is_empty());
    }

    #[test]
    fn hd_distance_covers_tile_centre() {
        let d = hd_dist_from_corner(200.0, 200.0);
        // point à TERRAIN_HD_DIST du centre, du côté opposé au coin
        let far = (100.0f32 * 2f32.sqrt()) + TERRAIN_HD_DIST;
        assert!(d >= far - 0.01);
    }
}
