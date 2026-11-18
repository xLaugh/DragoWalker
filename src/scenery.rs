//! Eau et effets de particules du créateur de terrain.
//!
//! Eau : des zones rectangulaires comme les WaterQuads du jeu (water.xml : minX, maxX, minY, maxY, z) ; dans
//! la vue elles sont dessinées par la même eau animée que la mer ; à l'export, DragoWalker écrit un water.xml
//! complet (l'eau du jeu + les nouvelles zones, pour ne pas perdre l'océan) chargé par un petit script client
//! (native FiveM LoadWaterFromPath).
//!
//! Effets : les effets d'ambiance que le jeu pose lui-même sur ses objets (CExtensionDefParticleEffect des
//! .ytyp : amb_barrel_fire, amb_steam_vent_round, amb_fly_swarm…), relevés dans les fichiers du jeu avec leur
//! échelle la plus courante. À l'export, chaque effet utilisé devient un type d'objet (dw_fx_…) qui porte
//! cette extension, avec un tout petit modèle invisible, placé là où on l'a mis.

use crate::app::DwApp;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaterZone {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub z: f32,
    /// eau claire (lagon) : plus transparente, turquoise, calme
    #[serde(default)]
    pub clear: bool,
}

/// Transparence de l'eau dans water.xml (a1..a4) : 26 pour presque toute l'eau du jeu, 22 sur une partie de celle
/// de Cayo Perico (eau claire), 80 dans le marais de Zancudo (eau trouble) — relevé dans water.xml et
/// water_heistisland.xml le 17 nov. 2026 ; plus bas = plus clair (**estimé**).
pub const WATER_ALPHA: u8 = 26;
pub const CLEAR_ALPHA: u8 = 12;
/// Amortissement des vagues d'une eau claire (CalmingQuads.fDampening : 0 à 0,6 autour de Cayo Perico) — **estimé**.
pub const CLEAR_CALM: f32 = 0.3;
/// Vagues vers les plages du terrain (WaveQuads.Amplitude : 1 à 1,5 le plus souvent dans le jeu).
pub const COAST_WAVE: f32 = 1.0;

/// Mer sur toute la carte, comme la ressource ls-removed (data/water.xml : une seule zone de -24000 à 24500 en x,
/// de -24000 à 28000 en y, à 0 m, transparence 26).
pub const SEA_ALL_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<WaterData>\n  <WaterQuads>\n  <Item>\n   <minX value=\"-24000\" />\n   <maxX value=\"24500\" />\n   <minY value=\"-24000\" />\n   <maxY value=\"28000\" />\n   <Type value=\"0\" />\n   <IsInvisible value=\"false\" />\n   <HasLimitedDepth value=\"false\" />\n   <z value=\"0\" />\n   <a1 value=\"26\" />\n   <a2 value=\"26\" />\n   <a3 value=\"26\" />\n   <a4 value=\"26\" />\n   <NoStencil value=\"false\" />\n  </Item>\n  </WaterQuads>\n  <CalmingQuads>\n  </CalmingQuads>\n  <WaveQuads>\n  </WaveQuads>\n</WaterData>\n";

/// Pas de la grille des zones d'eau (m). Dans water.xml et water_heistisland.xml du jeu, toutes les coordonnées
/// des zones (6 464 valeurs) sont des entiers pairs, presque toutes multiples de 4, et aucune zone ne fait moins de
/// 4 m : des coordonnées impaires (bords du terrain) faisaient geler le jeu au chargement de l'eau (17 nov. 2026).
pub const WATER_GRID: f32 = 4.0;

/// Rectangle d'eau calé sur la grille (vers l'extérieur), au moins une case de large.
pub fn snap_rect(min: [f32; 2], max: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let g = WATER_GRID;
    let (x0, y0) = ((min[0] / g).floor() * g, (min[1] / g).floor() * g);
    let (x1, y1) = (((max[0] / g).ceil() * g).max(x0 + g), ((max[1] / g).ceil() * g).max(y0 + g));
    ([x0, y0], [x1, y1])
}

/// Eau ajoutée autour des côtes du terrain : (quadrilatères d'eau, d'eau calme, de vagues) au format water.xml.
/// Mer : seulement si aucune eau du jeu ne recouvre déjà le terrain (sinon deux surfaces d'eau l'une sur l'autre) ;
/// cases de 32 m qui ont du fond sous 0 m, plus claires sur les hauts-fonds. Vagues : cases de 64 m traversées
/// par la côte, dirigées vers la terre (vers le haut de la pente), comme les vagues du jeu vers ses plages.
pub fn coast_water(t: &crate::terrain::Terrain, game: &[[f32; 5]]) -> (String, String, String, usize, usize) {
    let (mn, mx) = t.bounds();
    // bords calés sur la grille de l'eau du jeu (les cases de 32 et 64 m en sont des multiples)
    let (smin, smax) = snap_rect([mn.x, mn.y], [mx.x, mx.y]);
    let (mn, mx) = (v3(smin[0], smin[1], mn.z), v3(smax[0], smax[1], mx.z));
    let h = |x: f32, y: f32| t.height_at(x, y);
    let covered = game.iter().any(|q| q[0] < mx.x && q[2] > mn.x && q[1] < mx.y && q[3] > mn.y && q[4].abs() < 1.0);
    let (mut quads, mut calm, mut waves) = (String::new(), String::new(), String::new());
    let (mut nq, mut nw) = (0usize, 0usize);
    let cell_stats = |x0: f32, y0: f32, s: f32| -> (f32, f32, f32) {
        let (mut lo, mut hi, mut sum, mut n) = (f32::MAX, f32::MIN, 0.0f32, 0.0f32);
        for j in 0..=4 {
            for i in 0..=4 {
                if let Some(z) = h(x0 + s * i as f32 / 4.0, y0 + s * j as f32 / 4.0) {
                    lo = lo.min(z);
                    hi = hi.max(z);
                    sum += z;
                    n += 1.0;
                }
            }
        }
        (lo, hi, if n > 0.0 { sum / n } else { 0.0 })
    };
    if !covered {
        // grille de cases de 32 m : transparence de chacune (None = terre), puis cases voisines de même transparence
        // fusionnées en rectangles (le moins de zones d'eau possible : le jeu en a ~500 en tout)
        let s = 32.0;
        let (cols, rows) = (((mx.x - mn.x) / s).ceil().max(1.0) as usize, ((mx.y - mn.y) / s).ceil().max(1.0) as usize);
        let mut grid: Vec<Option<u8>> = vec![None; cols * rows];
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (mn.x + c as f32 * s, mn.y + r as f32 * s);
                let (x1, y1) = ((x + s).min(mx.x), (y + s).min(mx.y));
                let (lo, _, avg) = cell_stats(x, y, (x1 - x).min(y1 - y));
                if lo < -0.05 {
                    // une seule transparence, celle de la mer du jeu : des cases plus claires se voyaient en carrés en
                    // jeu ; l'eau peu profonde laisse déjà voir le sable (rendu du jeu)
                    let _ = avg;
                    grid[r * cols + c] = Some(WATER_ALPHA);
                }
            }
        }
        for (c0, c1, r0, r1, a) in merge_cells(&grid, cols, rows) {
            let (x, x1) = (mn.x + c0 as f32 * s, (mn.x + c1 as f32 * s).min(mx.x));
            let (y, y1) = (mn.y + r0 as f32 * s, (mn.y + r1 as f32 * s).min(mx.y));
            quads += &format!(
                "  <Item>\n   <minX value=\"{x:.0}\" />\n   <maxX value=\"{x1:.0}\" />\n   <minY value=\"{y:.0}\" />\n   <maxY value=\"{y1:.0}\" />\n   <Type value=\"0\" />\n   <IsInvisible value=\"false\" />\n   <HasLimitedDepth value=\"false\" />\n   <z value=\"0.0\" />\n   <a1 value=\"{a}\" />\n   <a2 value=\"{a}\" />\n   <a3 value=\"{a}\" />\n   <a4 value=\"{a}\" />\n   <NoStencil value=\"false\" />\n  </Item>\n"
            );
            nq += 1;
        }
    }
    let s = 64.0;
    let mut y = mn.y;
    while y < mx.y - 0.5 {
        let mut x = mn.x;
        while x < mx.x - 0.5 {
            let (x1, y1) = ((x + s).min(mx.x), (y + s).min(mx.y));
            let (lo, hi, _) = cell_stats(x, y, s.min(x1 - x).min(y1 - y));
            if lo < -0.3 && hi > 0.3 {
                let (cx, cy) = ((x + x1) * 0.5, (y + y1) * 0.5);
                let gx = h(cx + 12.0, cy).unwrap_or(0.0) - h(cx - 12.0, cy).unwrap_or(0.0);
                let gy = h(cx, cy + 12.0).unwrap_or(0.0) - h(cx, cy - 12.0).unwrap_or(0.0);
                let l = (gx * gx + gy * gy).sqrt();
                if l > 0.05 {
                    let (dx, dy) = (gx / l, gy / l);
                    waves += &format!(
                        "  <Item>\n   <minX value=\"{x:.0}\" />\n   <maxX value=\"{x1:.0}\" />\n   <minY value=\"{y:.0}\" />\n   <maxY value=\"{y1:.0}\" />\n   <Amplitude value=\"{COAST_WAVE}\" />\n   <XDirection value=\"{dx:.6}\" />\n   <YDirection value=\"{dy:.6}\" />\n  </Item>\n"
                    );
                    nw += 1;
                }
            }
            x += s;
        }
        y += s;
    }
    let _ = &mut calm;
    (quads, calm, waves, nq, nw)
}

/// Rectangles (colonnes c0..c1, lignes r0..r1, valeur) qui couvrent les cases remplies d'une grille : suites de
/// cases de même valeur sur chaque ligne, puis suites identiques des lignes suivantes empilées.
pub fn merge_cells(grid: &[Option<u8>], cols: usize, rows: usize) -> Vec<(usize, usize, usize, usize, u8)> {
    let mut runs: Vec<Vec<(usize, usize, u8)>> = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::new();
        let mut c = 0;
        while c < cols {
            match grid[r * cols + c] {
                Some(a) => {
                    let start = c;
                    while c < cols && grid[r * cols + c] == Some(a) {
                        c += 1;
                    }
                    row.push((start, c, a));
                }
                None => c += 1,
            }
        }
        runs.push(row);
    }
    let mut out = Vec::new();
    let mut used: Vec<Vec<bool>> = runs.iter().map(|r| vec![false; r.len()]).collect();
    for r in 0..rows {
        for k in 0..runs[r].len() {
            if used[r][k] {
                continue;
            }
            let run = runs[r][k];
            used[r][k] = true;
            let mut r1 = r + 1;
            while r1 < rows {
                match runs[r1].iter().position(|x| *x == run) {
                    Some(k1) if !used[r1][k1] => {
                        used[r1][k1] = true;
                        r1 += 1;
                    }
                    _ => break,
                }
            }
            out.push((run.0, run.1, r, r1, run.2));
        }
    }
    out
}

/// Eau calme (CalmingQuads) sur un rectangle.
pub fn calm_xml(min: [f32; 2], max: [f32; 2], damp: f32) -> String {
    format!("  <Item>\n   <minX value=\"{:.0}\" />\n   <maxX value=\"{:.0}\" />\n   <minY value=\"{:.0}\" />\n   <maxY value=\"{:.0}\" />\n   <fDampening value=\"{damp}\" />\n  </Item>\n", min[0], max[0], min[1], max[1])
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Emitter {
    pub pos: V3,
    pub fx: String,
    pub scale: f32,
}

/// Effet d'ambiance : nom dans le jeu, nom affiché, catégorie, échelle la plus courante dans les .ytyp du jeu.
pub struct Fx {
    pub name: &'static str,
    pub label: &'static str,
    pub cat: &'static str,
    pub scale: f32,
}

const fn fx(name: &'static str, label: &'static str, cat: &'static str, scale: f32) -> Fx {
    Fx { name, label, cat, scale }
}

/// Effets relevés dans les .ytyp du jeu (sonde DW_YTYP_FX, puis XML par CodeWalker) ; tous de type 0
/// (ambiant), probabilité 100, drapeaux 0, couleur 0xFFFFFFFF dans la grande majorité des cas.
pub const FX: [Fx; 30] = [
    fx("amb_beach_campfire", "Feu de camp", "Feu et fumée", 1.0),
    fx("amb_barrel_fire", "Feu de baril", "Feu et fumée", 0.1),
    fx("amb_fire_ring", "Cercle de feu", "Feu et fumée", 1.0),
    fx("amb_fbi_fire_small", "Petit incendie", "Feu et fumée", 1.0),
    fx("amb_fbi_fire_large", "Grand incendie", "Feu et fumée", 1.0),
    fx("amb_int_fireplace_sml", "Feu de cheminée", "Feu et fumée", 1.0),
    fx("amb_smoke_factory", "Fumée d'usine", "Feu et fumée", 0.8),
    fx("amb_smoke_gasworks", "Fumée de raffinerie", "Feu et fumée", 1.0),
    fx("amb_smoke_scrapyard", "Fumée de casse", "Feu et fumée", 1.0),
    fx("amb_generator_smoke", "Fumée de générateur", "Feu et fumée", 1.0),
    fx("amb_fbi_smoke_linger_hvy", "Fumée épaisse", "Feu et fumée", 1.0),
    fx("amb_steam_vent_round", "Vapeur (bouche ronde)", "Vapeur et brume", 1.0),
    fx("amb_steam_vent_open_lgt", "Vapeur légère", "Vapeur et brume", 1.0),
    fx("amb_vent_haze_lg", "Air chaud", "Vapeur et brume", 1.0),
    fx("amb_cold_air_vent", "Air froid", "Vapeur et brume", 1.0),
    fx("amb_dry_ice_area", "Fumée de scène", "Vapeur et brume", 1.0),
    fx("amb_river_mist_gen", "Brume de rivière", "Vapeur et brume", 1.0),
    fx("amb_fountain_double", "Fontaine", "Eau", 1.0),
    fx("amb_int_waterfall_splash", "Éclaboussures de cascade", "Eau", 1.0),
    fx("amb_water_roof_drips", "Gouttes du toit", "Eau", 1.0),
    fx("amb_drain_water", "Écoulement d'eau", "Eau", 1.0),
    fx("amb_sprinkler_golf", "Arroseur", "Eau", 1.0),
    fx("amb_falling_leaves_l", "Feuilles qui tombent", "Nature", 1.0),
    fx("amb_falling_leaves_palm", "Palmes qui tombent", "Nature", 0.5),
    fx("amb_butterflys", "Papillons", "Nature", 1.0),
    fx("amb_fly_swarm", "Mouches", "Nature", 1.0),
    fx("amb_wind_dust_swirl", "Tourbillon de poussière", "Nature", 1.0),
    fx("amb_wind_leaves", "Feuilles au vent", "Nature", 1.0),
    fx("amb_electric_crackle", "Grésillement électrique", "Électricité", 1.0),
    fx("amb_sparking_wires", "Câbles qui crépitent", "Électricité", 1.0),
];

fn fx_color(cat: &str) -> Color32 {
    match cat {
        "Feu et fumée" => Color32::from_rgb(255, 120, 40),
        "Vapeur et brume" => Color32::from_rgb(220, 220, 230),
        "Eau" => Color32::from_rgb(70, 160, 255),
        "Nature" => Color32::from_rgb(120, 220, 90),
        _ => Color32::from_rgb(255, 230, 80),
    }
}

/// Allure des particules d'un effet (aperçu dans DragoWalker ; le vrai effet est celui du jeu).
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Fire,
    Smoke,
    Steam,
    Fountain,
    Drips,
    Leaves,
    Insects,
    Dust,
    Sparks,
}

fn style_of(name: &str) -> Style {
    let has = |k: &[&str]| k.iter().any(|w| name.contains(w));
    if has(&["fire", "campfire", "fireplace"]) {
        Style::Fire
    } else if has(&["smoke"]) {
        Style::Smoke
    } else if has(&["steam", "haze", "air_vent", "dry_ice", "mist"]) {
        Style::Steam
    } else if has(&["fountain", "sprinkler", "waterfall"]) {
        Style::Fountain
    } else if has(&["drip", "drain"]) {
        Style::Drips
    } else if has(&["leaves"]) {
        Style::Leaves
    } else if has(&["butterfl", "fly", "insect", "moth"]) {
        Style::Insects
    } else if has(&["dust", "wind"]) {
        Style::Dust
    } else {
        Style::Sparks
    }
}

fn h01(i: u32, s: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ s.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Dessine l'effet animé : `base` = point d'émission à l'écran, `px` = pixels par mètre, `t` = temps (s).
pub fn draw_fx(painter: &egui::Painter, base: Pos2, px: f32, fx: &str, t: f32) {
    let st = style_of(fx);
    let pt = |x: f32, y: f32| base + egui::vec2(x * px, -y * px);
    let rgba = |r: u8, g: u8, b: u8, a: f32| Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8);
    match st {
        Style::Fire => {
            for i in 0..22 {
                let ph = (t * 1.6 + h01(i, 1)).fract();
                let x = (h01(i, 2) - 0.5) * 0.7 * (1.0 - ph);
                let r = (0.28 * (1.0 - ph) + 0.05) * px;
                let (cr, cg, cb) = if ph < 0.35 { (255, 230, 90) } else if ph < 0.7 { (255, 130, 30) } else { (190, 45, 20) };
                painter.circle_filled(pt(x, ph * 1.7), r.max(1.0), rgba(cr, cg, cb, 0.9 * (1.0 - ph)));
            }
        }
        Style::Smoke | Style::Steam => {
            let (g, a, h, sp) = if st == Style::Smoke { (120u8, 0.55, 4.5, 0.35) } else { (235u8, 0.45, 2.8, 0.45) };
            for i in 0..16 {
                let ph = (t * sp + h01(i, 3)).fract();
                let x = (h01(i, 4) - 0.5) * 0.8 + ph * 0.7;
                let r = (0.25 + ph * 1.1) * px;
                painter.circle_filled(pt(x, ph * h), r.max(1.5), rgba(g, g, g.saturating_add(5), a * (1.0 - ph)));
            }
        }
        Style::Fountain => {
            for i in 0..28 {
                let ph = (t * 0.9 + h01(i, 5)).fract();
                let vx = (h01(i, 6) - 0.5) * 1.6;
                let y = (3.0 * ph - 3.0 * ph * ph) * 1.6;
                painter.circle_filled(pt(vx * ph * 1.3, y), (0.07 * px).max(1.0), rgba(130, 195, 255, 0.85));
            }
        }
        Style::Drips => {
            for i in 0..12 {
                let ph = (t * 0.8 + h01(i, 7)).fract();
                painter.circle_filled(pt((h01(i, 8) - 0.5) * 1.6, 2.0 - ph * 2.0), (0.05 * px).max(1.0), rgba(130, 195, 255, 0.9));
            }
        }
        Style::Leaves => {
            for i in 0..12 {
                let ph = (t * 0.22 + h01(i, 9)).fract();
                let x = (h01(i, 10) - 0.5) * 2.4 + (t * 2.0 + h01(i, 11) * 6.0).sin() * 0.45;
                let c = if i % 3 == 0 { (150, 110, 50) } else { (110, 160, 60) };
                painter.circle_filled(pt(x, 3.2 - ph * 3.2), (0.12 * px).max(1.5), rgba(c.0, c.1, c.2, 0.95));
            }
        }
        Style::Insects => {
            for i in 0..14 {
                let a = t * (2.0 + h01(i, 12) * 3.0) + h01(i, 13) * 6.3;
                let rad = 0.4 + h01(i, 14) * 0.7;
                let col = if fx.contains("butterfl") { (255, 200, 80) } else { (25, 25, 25) };
                painter.circle_filled(pt(a.cos() * rad, 1.0 + (a * 1.7).sin() * 0.35), (0.05 * px).max(1.2), rgba(col.0, col.1, col.2, 0.95));
            }
        }
        Style::Dust => {
            for i in 0..20 {
                let ph = (t * 0.4 + h01(i, 15)).fract();
                let a = ph * 6.0 + h01(i, 16) * 6.3;
                let rad = 0.3 + ph * 1.3;
                painter.circle_filled(pt(a.cos() * rad, ph * 2.2), (0.08 * px).max(1.0), rgba(200, 175, 130, 0.6 * (1.0 - ph)));
            }
        }
        Style::Sparks => {
            let step = (t * 12.0).floor() as u32;
            for i in 0..14 {
                if h01(i, step) < 0.55 {
                    continue;
                }
                let a = h01(i, step + 99) * 6.3;
                let l = 0.15 + h01(i, step + 7) * 0.35;
                painter.line_segment([pt(0.0, 0.3), pt(a.cos() * l, 0.3 + a.sin().abs() * l)], egui::Stroke::new(1.5_f32, rgba(255, 245, 170, 0.95)));
            }
        }
    }
}

/// Nom du type d'objet exporté pour un effet.
pub fn fx_arch(name: &str) -> String {
    format!("dw_fx_{}", name.trim_start_matches("amb_"))
}

impl DwApp {
    /// Outil Eau : glisser un rectangle ; niveau = hauteur du point de départ + 0,2 m ; si « creuser », le sol
    /// est descendu sous l'eau à l'intérieur (raccord en pente douce sur 4 m).
    pub fn water_tool(&mut self, resp: &egui::Response, g: Option<V3>) {
        use eframe::egui::PointerButton;
        if resp.drag_started_by(PointerButton::Primary) {
            self.terrain.water_start = g;
        }
        if resp.drag_stopped() {
            let (Some(a), Some(b)) = (self.terrain.water_start.take(), g) else { return };
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
                return;
            }
            self.push_undo();
            // arrondi au décimètre : niveau lisible dans la liste des zones
            let z = ((a.z + 0.2) * 10.0).round() / 10.0;
            let depth = self.terrain.water_depth;
            if let Some(t) = self.proj.terrain.as_mut() {
                if self.terrain.water_dig {
                    for j in 0..t.ny {
                        for i in 0..t.nx {
                            let p = v3(t.origin.x + i as f32 * t.cell, t.origin.y + j as f32 * t.cell, 0.0);
                            // distance au bord (négative dedans)
                            let dx = (x0 - p.x).max(p.x - x1);
                            let dy = (y0 - p.y).max(p.y - y1);
                            let d = dx.max(dy);
                            if d > 4.0 {
                                continue;
                            }
                            let k = if d <= 0.0 { (-d / 4.0).min(1.0) } else { 0.0 };
                            let target = z - 0.3 - depth * k;
                            let h = &mut std::sync::Arc::make_mut(&mut t.h)[j * t.nx + i];
                            if *h > target {
                                *h = if d <= 0.0 { target } else { *h + (target - *h) * (1.0 - d / 4.0) * 0.5 };
                            }
                        }
                    }
                }
                t.waters.push(WaterZone { min: [x0, y0], max: [x1, y1], z, clear: self.terrain.water_clear });
            }
            self.terrain.dirty = true;
            self.status = format!("Eau ajoutée : {:.0} × {:.0} m à {:.1} m de haut.", x1 - x0, y1 - y0, z);
        }
    }

    /// Outil Effets : clic = poser l'effet choisi ; Ctrl + clic = retirer l'effet le plus proche (5 m).
    pub fn fx_tool(&mut self, resp: &egui::Response, g: Option<V3>, ctrl: bool) {
        if !resp.clicked_by(eframe::egui::PointerButton::Primary) {
            return;
        }
        let Some(p) = g else { return };
        self.push_undo();
        let f = &FX[self.terrain.fx.min(FX.len() - 1)];
        let scale = self.terrain.fx_scale * f.scale;
        let Some(t) = self.proj.terrain.as_mut() else { return };
        if ctrl {
            if let Some((i, _)) = t.emitters.iter().enumerate().map(|(i, e)| (i, (e.pos - p).len())).filter(|x| x.1 < 5.0).min_by(|a, b| a.1.total_cmp(&b.1)) {
                let e = t.emitters.remove(i);
                self.status = format!("Effet « {} » retiré.", e.fx);
            }
            return;
        }
        t.emitters.push(Emitter { pos: p + v3(0.0, 0.0, 0.3), fx: f.name.to_string(), scale });
        self.status = format!("Effet « {} » posé ({} au total) · Ctrl + clic pour en retirer un.", f.label, t.emitters.len());
    }

    /// Effets posés : pastilles de couleur dans la vue (nom au survol de près), zone d'eau en cours de tracé.
    pub fn draw_scenery_overlay(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        let time = painter.ctx().input(|i| i.time) as f32;
        for e in &t.emitters {
            let Some((q, _)) = self.cam.project(e.pos) else { continue };
            let cat = FX.iter().find(|f| f.name == e.fx).map(|f| f.cat).unwrap_or("");
            let col = fx_color(cat);
            // échelle : pixels pour un mètre à cet endroit ; l'effet est animé comme en jeu (aperçu)
            let px = self.cam.project(e.pos + v3(0.0, 0.0, 1.0)).map(|u| (u.0 - q).length()).unwrap_or(0.0);
            let base_scale = FX.iter().find(|f| f.name == e.fx).map(|f| f.scale).unwrap_or(1.0).max(1e-3);
            let k = (e.scale / base_scale).clamp(0.2, 4.0);
            if px > 0.5 && (e.pos - self.cam.eye).len() < 400.0 {
                draw_fx(painter, q, px * k, &e.fx, time);
            }
            painter.circle_filled(q, 3.5, col);
            if hover.map(|h| h.distance(q) < 40.0).unwrap_or(false) || (e.pos - self.cam.eye).len() < 40.0 {
                let label = FX.iter().find(|f| f.name == e.fx).map(|f| f.label).unwrap_or(&e.fx);
                painter.text(q + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        if self.tool == crate::app::Tool::Terrain && self.terrain.brush == crate::terrain::Brush::Water {
            if let (Some(a), Some(h)) = (self.terrain.water_start, hover) {
                if let Some(b) = self.terrain_ground(h) {
                    let z = a.z + 0.2;
                    let c = [v3(a.x, a.y, z), v3(b.x, a.y, z), v3(b.x, b.y, z), v3(a.x, b.y, z)];
                    let pts: Vec<Pos2> = c.iter().filter_map(|p| self.cam.project(*p).map(|q| q.0)).collect();
                    if pts.len() == 4 {
                        painter.add(egui::Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(60, 140, 230, 90), egui::Stroke::new(2.0_f32, Color32::from_rgb(90, 170, 255))));
                    }
                }
            }
        }
    }

    pub fn terrain_ui_water(&mut self, ui: &mut egui::Ui) {
        ui.label("Glisser un rectangle dans la vue : l'eau monte à la hauteur du point de départ (+0,2 m).");
        ui.horizontal(|ui| {
            ui.label("Nouvelle zone :");
            ui.selectable_value(&mut self.terrain.water_clear, false, "Eau normale");
            ui.selectable_value(&mut self.terrain.water_clear, true, "Eau claire (lagon)").on_hover_text("Turquoise, transparente et calme : lagon, crique, piscine naturelle");
        });
        ui.checkbox(&mut self.terrain.water_dig, "Creuser le bassin sous l'eau");
        if self.terrain.water_dig {
            ui.add(egui::Slider::new(&mut self.terrain.water_depth, 0.5..=10.0).text("profondeur").suffix(" m"));
        }
        if let Some(t) = self.proj.terrain.as_mut() {
            ui.checkbox(&mut t.sea_all, "Mer partout (carte vide, ls-removed)").on_hover_text("L'eau exportée part d'une mer qui couvre toute la carte (comme la ressource ls-removed) au lieu de l'eau du jeu : là où Los Santos est retiré, c'est la mer. Coché tout seul pour les tests en jeu quand ls-removed est actif.");
            ui.checkbox(&mut t.coast, "Côtes : eau claire et vagues vers les plages").on_hover_text("À l'export : là où le terrain descend sous la mer, de l'eau (si le jeu n'en a pas déjà à cet endroit), plus claire sur les hauts-fonds, et des vagues qui vont vers la plage. Dans la vue : eau turquoise peu profonde et écume sur le sable.");
        }
        let Some(t) = self.proj.terrain.as_ref() else { return };
        if !t.waters.is_empty() {
            ui.separator();
            ui.label(RichText::new("Zones d'eau").strong());
        }
        let mut clear_set: Option<(usize, bool)> = None;
        let mut del = None;
        let mut changed = false;
        let mut zs: Vec<f32> = t.waters.iter().map(|w| w.z).collect();
        for (i, w) in t.waters.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{:.0} × {:.0} m", w.max[0] - w.min[0], w.max[1] - w.min[1]));
                if ui.add(egui::DragValue::new(&mut zs[i]).speed(0.05).max_decimals(2).suffix(" m")).on_hover_text("Niveau de l'eau").changed() {
                    changed = true;
                }
                let mut c = w.clear;
                if ui.checkbox(&mut c, "claire").on_hover_text("Eau claire (lagon) : turquoise, transparente, calme").changed() {
                    clear_set = Some((i, c));
                }
                if ui.small_button("Supprimer").clicked() {
                    del = Some(i);
                }
            });
        }
        if changed {
            // un pas d'annulation par glisser du niveau
            self.undo_once(ui.ctx());
            if let Some(t) = self.proj.terrain.as_mut() {
                for (w, z) in t.waters.iter_mut().zip(zs) {
                    w.z = z;
                }
            }
        }
        if let Some((i, c)) = clear_set {
            self.push_undo();
            if let Some(t) = self.proj.terrain.as_mut() {
                t.waters[i].clear = c;
            }
        }
        if let Some(i) = del {
            self.push_undo();
            if let Some(t) = self.proj.terrain.as_mut() {
                t.waters.remove(i);
            }
        }
        ui.label(RichText::new("En jeu : un water.xml (eau du jeu + tes zones) est chargé par un petit script client, l'eau est nageable.").small().weak());
    }

    pub fn terrain_ui_fx(&mut self, ui: &mut egui::Ui) {
        ui.label("Clic dans la vue : poser l'effet choisi · Ctrl + clic : retirer le plus proche.");
        let time = ui.ctx().input(|i| i.time) as f32;
        let cats: Vec<&str> = FX.iter().fold(Vec::new(), |mut v, f| {
            if !v.contains(&f.cat) {
                v.push(f.cat);
            }
            v
        });
        for cat in cats {
            ui.label(RichText::new(cat).strong().color(fx_color(cat)));
            ui.horizontal_wrapped(|ui| {
                for (i, f) in FX.iter().enumerate().filter(|(_, f)| f.cat == cat) {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(104.0, 98.0), egui::Sense::click());
                    let on = self.terrain.fx == i;
                    let p = ui.painter_at(rect);
                    let img = egui::Rect::from_min_size(rect.min, egui::vec2(104.0, 80.0));
                    p.rect_filled(img, 6.0, if resp.hovered() { Color32::from_rgb(40, 46, 56) } else { Color32::from_rgb(24, 27, 33) });
                    // sol + effet animé
                    p.line_segment([egui::pos2(img.left() + 8.0, img.bottom() - 12.0), egui::pos2(img.right() - 8.0, img.bottom() - 12.0)], egui::Stroke::new(1.0_f32, Color32::from_gray(70)));
                    draw_fx(&p, egui::pos2(img.center().x, img.bottom() - 12.0), if matches!(style_of(f.name), Style::Fire | Style::Insects | Style::Sparks | Style::Drips) { 24.0 } else { 13.0 }, f.name, time + i as f32);
                    if on {
                        p.rect_stroke(img, 6.0, egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 170, 60)), egui::StrokeKind::Inside);
                    }
                    p.text(egui::pos2(rect.center().x, img.bottom() + 2.0), egui::Align2::CENTER_TOP, f.label, egui::FontId::proportional(11.0), Color32::from_gray(210));
                    if resp.on_hover_text(f.name).clicked() {
                        self.terrain.fx = i;
                        self.set_terrain_brush(crate::terrain::Brush::Particles);
                    }
                }
            });
        }
        ui.ctx().request_repaint();
        ui.add(egui::Slider::new(&mut self.terrain.fx_scale, 0.2..=4.0).text("taille"));
        if let Some(t) = self.proj.terrain.as_ref() {
            ui.label(RichText::new(format!("{} effet(s) posé(s)", t.emitters.len())).small().weak());
        }
    }

    /// Export : water.xml complet + script client, et les effets (types d'objets dw_fx_* + modèle invisible).
    /// Renvoie (types d'objets XML, entités XML, étendue des effets, eau laissée de côté) à ajouter au ytyp / ymap
    /// du terrain. Eau laissée de côté = le terrain a des zones d'eau mais le water.xml du jeu n'est pas lisible
    /// (jeu non chargé) : sans lui, l'océan disparaîtrait en jeu, donc rien n'est écrit.
    pub fn export_scenery(&self, stream: &std::path::Path, tiles: &mut u32) -> Result<(Vec<String>, String, Option<(V3, V3)>, bool), String> {
        let Some(t) = self.proj.terrain.as_ref() else { return Ok((Vec::new(), String::new(), None, false)) };
        // ressource FiveM : les modèles vont dans <ressource>/stream, l'eau et son script à la racine de la
        // ressource ; export XML : tout dans le dossier choisi
        let in_resource = stream.file_name().is_some_and(|n| n.eq_ignore_ascii_case("stream"));
        let root = if in_resource { stream.parent().unwrap_or(stream) } else { stream };
        let mut archs = Vec::new();
        let mut ents = String::new();
        let mut ext: Option<(V3, V3)> = None;
        // eau : le water.xml du jeu (océan, lacs) + les zones du projet + les côtes du terrain
        let has_coast = t.coast && t.h.iter().any(|z| *z < -0.3) && t.h.iter().any(|z| *z > 0.3);
        let sea_all = t.sea_all || self.export_sea_all;
        let base = if t.waters.is_empty() && !has_coast {
            None
        } else if sea_all {
            Some(SEA_ALL_XML.as_bytes().to_vec())
        } else {
            self.game.world.as_ref().and_then(|w| {
                w.vfs.files.iter().find(|f| f.name.eq_ignore_ascii_case("water.xml") && f.path().to_lowercase().contains("levels\\gta5")).and_then(|f| w.vfs.read_binary(f, &w.keys))
            })
        };
        let water_skipped = !t.waters.is_empty() && base.is_none();
        if let Some(base) = base {
            let mut s = String::from_utf8_lossy(&base).to_string();
            let mut items = String::new();
            let mut calm = String::new();
            for w in &t.waters {
                let a = if w.clear { CLEAR_ALPHA } else { WATER_ALPHA };
                items += &format!(
                    "  <Item>\n   <minX value=\"{:.0}\" />\n   <maxX value=\"{:.0}\" />\n   <minY value=\"{:.0}\" />\n   <maxY value=\"{:.0}\" />\n   <Type value=\"0\" />\n   <IsInvisible value=\"false\" />\n   <HasLimitedDepth value=\"false\" />\n   <z value=\"{:.2}\" />\n   <a1 value=\"{a}\" />\n   <a2 value=\"{a}\" />\n   <a3 value=\"{a}\" />\n   <a4 value=\"{a}\" />\n   <NoStencil value=\"false\" />\n  </Item>\n",
                    snap_rect(w.min, w.max).0[0], snap_rect(w.min, w.max).1[0], snap_rect(w.min, w.max).0[1], snap_rect(w.min, w.max).1[1], w.z
                );
                if w.clear {
                    let (a, b) = snap_rect(w.min, w.max);
                    calm += &calm_xml(a, b, CLEAR_CALM);
                }
            }
            let mut waves = String::new();
            if has_coast {
                let game = crate::gta::world::parse_water(&s);
                let (q, c, wv, nq, nw) = coast_water(t, &game);
                items += &q;
                calm += &c;
                waves = wv;
                if std::env::var("DW_TEST_AI").is_ok() || std::env::var("DW_TEST_TERRAIN_EXPORT").is_ok() {
                    println!("côtes : {nq} zone(s) de mer, {nw} zone(s) de vagues");
                }
            }
            match s.find("</WaterQuads>") {
                Some(p) => s.insert_str(p, &items),
                None => return Err("water.xml du jeu illisible".into()),
            }
            for (tag, add) in [("</CalmingQuads>", &calm), ("</WaveQuads>", &waves)] {
                if add.is_empty() {
                    continue;
                }
                match s.find(tag) {
                    Some(p) => s.insert_str(p, add),
                    None => return Err("water.xml du jeu illisible".into()),
                }
            }
            std::fs::write(root.join("dw_water.xml"), s).map_err(|e| e.to_string())?;
            std::fs::write(
                root.join("dw_water.lua"),
                "-- DragoWalker : charge l'eau du terrain (l'eau du jeu + les nouvelles zones), une fois le joueur dans la partie\n-- (pas pendant le chargement du jeu)\nCreateThread(function()\n    while not NetworkIsPlayerActive(PlayerId()) do Wait(500) end\n    Wait(2000)\n    LoadWaterFromPath(GetCurrentResourceName(), 'dw_water.xml')\nend)\n",
            )
            .map_err(|e| e.to_string())?;
        }
        // effets
        if !t.emitters.is_empty() {
            // modèle presque invisible : un minuscule triangle sous le point
            use crate::gta::drawable::{RGeom, RModel, RVert};
            let m = RModel {
                verts: vec![
                    RVert { pos: [0.0, 0.0, -0.05], nrm: [0.0, 0.0, 1.0], uv: [0.0, 0.0], col: [255; 4], uv1: [0.0, 0.0] },
                    RVert { pos: [0.01, 0.0, -0.05], nrm: [0.0, 0.0, 1.0], uv: [1.0, 0.0], col: [255; 4], uv1: [0.0, 0.0] },
                    RVert { pos: [0.0, 0.01, -0.05], nrm: [0.0, 0.0, 1.0], uv: [0.0, 1.0], col: [255; 4], uv1: [0.0, 0.0] },
                ],
                idx: vec![0, 1, 2],
                geoms: vec![RGeom { first: 0, count: 3, tex: 0, cutout: false, blend: 0, glass: false, ..Default::default() }],
                embedded_dict: None,
            };
            crate::sollumz::write_ydr_xml_ex(stream, "dw_fx_point", &m, &[0], &[], Some(crate::terrain::TERRAIN_TXD))?;
            let mut used: Vec<(String, f32)> = Vec::new();
            for e in &t.emitters {
                if !used.iter().any(|u| u.0 == e.fx && (u.1 - e.scale).abs() < 1e-3) {
                    used.push((e.fx.clone(), e.scale));
                }
            }
            for (fxn, sc) in &used {
                let name = format!("{}_{}", fx_arch(fxn), (sc * 100.0).round() as u32);
                archs.push(format!(
                    "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"150\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"-1\" y=\"-1\" z=\"-1\" />\n   <bbMax x=\"1\" y=\"1\" z=\"1\" />\n   <bsCentre x=\"0\" y=\"0\" z=\"0\" />\n   <bsRadius value=\"1.8\" />\n   <hdTextureDist value=\"50\" />\n   <name>{name}</name>\n   <textureDictionary />\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>dw_fx_point</assetName>\n   <extensions>\n    <Item type=\"CExtensionDefParticleEffect\">\n     <name>{name}</name>\n     <offsetPosition x=\"0\" y=\"0\" z=\"0\" />\n     <offsetRotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n     <fxName>{fxn}</fxName>\n     <fxType value=\"0\" />\n     <boneTag value=\"0\" />\n     <scale value=\"{sc}\" />\n     <probability value=\"100\" />\n     <flags value=\"0\" />\n     <color value=\"0xFFFFFFFF\" />\n    </Item>\n   </extensions>\n  </Item>"
                ));
            }
            for e in &t.emitters {
                *tiles += 1;
                let name = format!("{}_{}", fx_arch(&e.fx), (e.scale * 100.0).round() as u32);
                ents += &format!(
                    "  <Item type=\"CEntityDef\">\n   <archetypeName>{name}</archetypeName>\n   <flags value=\"{}\" />\n   <guid value=\"{}\" />\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"150\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>LODTYPES_DEPTH_HD</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
                    crate::terrain::TERRAIN_ENTITY_FLAGS,
                    0x0D77_0000u32 + *tiles,
                    e.pos.x,
                    e.pos.y,
                    e.pos.z
                );
                let (a, b) = (e.pos - v3(150.0, 150.0, 150.0), e.pos + v3(150.0, 150.0, 150.0));
                ext = Some(match ext {
                    Some((mn, mx)) => (mn.min(a), mx.max(b)),
                    None => (a, b),
                });
            }
        }
        Ok((archs, ents, ext, water_skipped))
    }
}

#[cfg(test)]
mod tests_water {
    use super::*;

    #[test]
    fn grille_de_l_eau() {
        let (a, b) = snap_rect([-61.3, 131.0], [195.0, 133.0]);
        assert_eq!((a, b), ([-64.0, 128.0], [196.0, 136.0]));
        for v in [a[0], a[1], b[0], b[1]] {
            assert_eq!(v % 4.0, 0.0);
        }
        // terrain dont les bords tombent hors de la grille : toutes les zones exportées sont sur la grille
        let mut t = crate::terrain::Terrain::new(v3(195.3, -934.7, 0.0), 512.0, 2.0, 0);
        t.generate_ex(&crate::terrain::GenSpec { preset: 1, island: true, ..Default::default() });
        let (q, _, w, nq, nw) = coast_water(&t, &[]);
        assert!(nq > 0 && nw > 0);
        for v in regex_vals(&q).into_iter().chain(regex_vals(&w)) {
            assert_eq!(v % 4.0, 0.0, "{v}");
        }
    }

    fn regex_vals(s: &str) -> Vec<f32> {
        let mut out = Vec::new();
        for tag in ["minX", "maxX", "minY", "maxY"] {
            for part in s.split(&format!("<{tag} value=\"")).skip(1) {
                out.push(part.split('"').next().unwrap().parse().unwrap());
            }
        }
        out
    }

    #[test]
    fn fusion_des_cases() {
        // 4 × 3 : un bloc de 2 × 2 (12), une ligne de 26, une case seule
        let g = [Some(12), Some(12), None, Some(26), Some(12), Some(12), None, None, Some(26), Some(26), Some(26), Some(26)].map(|x: Option<u8>| x);
        let m = merge_cells(&g, 4, 3);
        let cover: usize = m.iter().map(|(c0, c1, r0, r1, _)| (c1 - c0) * (r1 - r0)).sum();
        assert_eq!(cover, g.iter().filter(|x| x.is_some()).count());
        assert_eq!(m.len(), 3, "{m:?}");
    }
}
