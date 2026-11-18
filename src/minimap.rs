//! Minimap et carte de pause : le terrain du projet peint dans les tuiles de la carte du jeu.
//!
//! La carte du jeu est faite de 6 tuiles (x64b.rpf\data\cdimages\scaleform_generic.rpf) : minimap_R_C.ytd (une
//! texture du même nom, 512 × 512, DXT1, un seul niveau, Usage DEFAULT) et minimap_sea_R_C.ytd (1024 × 1024, DXT5,
//! la même image avec la mer profonde transparente), R = rangée 0..2 du nord au sud, C = colonne 0..1 d'ouest en
//! est. Chaque tuile couvre 4 500 m, la grille part de (-4140, 8400) (coin nord-ouest) — constantes de la ressource
//! extra-map-tiles (alexlicuriceanu), recoupées avec l'image de minimap_1_0 (collines de Vinewood, haut de la ville).
//! À l'export, une copie des tuiles touchées par le terrain est repeinte dans le style du jeu (terre gris foncé,
//! mer plus claire près des côtes, routes gris clair) et placée dans stream/ : FiveM la charge à la place de
//! l'originale (comme les cartes Roxwood ou Paleto), pour le radar et la carte de pause. Les routes vectorielles
//! du radar très zoomé (minimap_*.ydd) ne sont pas modifiées.

use crate::gta::drawable::{TexData, TexFmt};
use crate::math::V3;

pub const START_X: f32 = -4140.0;
pub const START_Y: f32 = 8400.0;
pub const TILE: f32 = 4500.0;
pub const ROWS: usize = 3;
pub const COLS: usize = 2;

/// Couleurs du style du jeu (relevées sur minimap_1_0) : terre, routes, mer près de la côte.
const LAND: [u8; 3] = [30, 30, 30];
const ROAD: [u8; 3] = [178, 178, 178];
const SHORE: [u8; 3] = [208, 218, 222];

/// Rectangle du monde d'une tuile : (x0, y0, x1, y1).
pub fn tile_rect(r: usize, c: usize) -> (f32, f32, f32, f32) {
    let x0 = START_X + c as f32 * TILE;
    let y1 = START_Y - r as f32 * TILE;
    (x0, y1 - TILE, x0 + TILE, y1)
}

/// Tuiles touchées par un rectangle du monde.
pub fn tiles_for(mn: V3, mx: V3) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            let (x0, y0, x1, y1) = tile_rect(r, c);
            if mn.x < x1 && mx.x > x0 && mn.y < y1 && mx.y > y0 {
                out.push((r, c));
            }
        }
    }
    out
}

/// Repeint le terrain dans une tuile (image RGBA `n` × `n`, nord en haut). `sea` : tuile « sea » (mer profonde
/// transparente) ; `deep` : couleur de la mer au large prise dans l'image du jeu ; `sea_all` : toute la tuile en
/// mer d'abord (carte du jeu retirée, comme avec ls-removed).
pub fn paint_tile(px: &mut [u8], n: usize, r: usize, c: usize, t: &crate::terrain::Terrain, sea: bool, deep: [u8; 4], sea_all: bool) {
    let (x0, y0, x1, y1) = tile_rect(r, c);
    let m_per_px = TILE / n as f32;
    let set = |px: &mut [u8], i: usize, j: usize, col: [u8; 4]| {
        let k = (j * n + i) * 4;
        px[k..k + 4].copy_from_slice(&col);
    };
    if sea_all {
        for j in 0..n {
            for i in 0..n {
                set(px, i, j, if sea { [deep[0], deep[1], deep[2], 0] } else { deep });
            }
        }
    }
    let (mn, mx) = t.bounds();
    let to_px = |x: f32, y: f32| -> (f32, f32) { ((x - x0) / m_per_px, (y1 - y) / m_per_px) };
    let (a, b) = (to_px(mn.x, mx.y), to_px(mx.x, mn.y));
    let (i0, i1) = ((a.0.floor().max(0.0)) as usize, (b.0.ceil().min(n as f32)) as usize);
    let (j0, j1) = ((a.1.floor().max(0.0)) as usize, (b.1.ceil().min(n as f32)) as usize);
    let _ = y0;
    let _ = x1;
    for j in j0..j1 {
        for i in i0..i1 {
            let (x, y) = (x0 + (i as f32 + 0.5) * m_per_px, y1 - (j as f32 + 0.5) * m_per_px);
            let Some(h) = t.height_at(x, y) else { continue };
            let col = if h >= 0.0 {
                [LAND[0], LAND[1], LAND[2], 255]
            } else {
                // mer : claire près de la côte, comme au large plus loin (12 m de fond)
                let k = (-h / 12.0).clamp(0.0, 1.0);
                let mix = |s: u8, d: u8| (s as f32 + (d as f32 - s as f32) * k) as u8;
                let a = if sea { (255.0 * (1.0 - k)) as u8 } else { 255 };
                [mix(SHORE[0], deep[0]), mix(SHORE[1], deep[1]), mix(SHORE[2], deep[2]), a]
            };
            set(px, i, j, col);
        }
    }
    // routes : traits gris clair, au moins un pixel de large
    for road in &t.roads {
        let w = (road.kind().width / m_per_px).max(1.0);
        for s in road.samples.windows(2) {
            let (p, q) = (to_px(s[0][0], s[0][1]), to_px(s[1][0], s[1][1]));
            let steps = ((q.0 - p.0).abs().max((q.1 - p.1).abs()) * 2.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let f = k as f32 / steps as f32;
                let (cx, cy) = (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
                let rr = (w * 0.5).max(0.5);
                let (ia, ib) = ((cx - rr).floor().max(0.0) as usize, ((cx + rr).ceil() as usize).min(n));
                let (ja, jb) = ((cy - rr).floor().max(0.0) as usize, ((cy + rr).ceil() as usize).min(n));
                for jj in ja..jb {
                    for ii in ia..ib {
                        let (dx, dy) = (ii as f32 + 0.5 - cx, jj as f32 + 0.5 - cy);
                        if dx * dx + dy * dy <= rr * rr + 0.25 {
                            set(px, ii, jj, [ROAD[0], ROAD[1], ROAD[2], 255]);
                        }
                    }
                }
            }
        }
    }
}

/// Dictionnaire de textures (XML de CodeWalker) d'une tuile : une texture du nom du fichier, un seul niveau, comme
/// celles du jeu (Usage DEFAULT), en A8R8G8B8.
fn write_tile_ytd(dir: &std::path::Path, name: &str, n: usize, px: Vec<u8>) -> Result<(), String> {
    let tex_dir = dir.join(name);
    std::fs::create_dir_all(&tex_dir).map_err(|e| e.to_string())?;
    let t = TexData { w: n as u32, h: n as u32, fmt: TexFmt::Rgba8, levels: vec![px] };
    std::fs::write(tex_dir.join(format!("{name}.dds")), crate::sollumz::write_dds(&t)).map_err(|e| e.to_string())?;
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<TextureDictionary>\n <Item>\n  <Name>{name}</Name>\n  <Unk32 value=\"0\" />\n  <Usage>DEFAULT</Usage>\n  <UsageFlags />\n  <ExtraFlags value=\"0\" />\n  <Width value=\"{n}\" />\n  <Height value=\"{n}\" />\n  <MipLevels value=\"1\" />\n  <Format>D3DFMT_A8R8G8B8</Format>\n  <FileName>{name}.dds</FileName>\n </Item>\n</TextureDictionary>\n"
    );
    std::fs::write(dir.join(format!("{name}.ytd.xml")), xml).map_err(|e| e.to_string())
}

/// Tuile du jeu décodée (RGBA), et sa taille.
fn game_tile(w: &crate::gta::world::GameWorld, name: &str) -> Option<(Vec<u8>, usize)> {
    let file = format!("{name}.ytd");
    let f = w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case(&file)).last()?;
    let r = w.vfs.read_resource(f, &w.keys)?;
    let t = crate::gta::drawable::find_texture(&r, 0, crate::gta::crypto::joaat(name), 4096)?;
    let d = crate::texdecode::decode_level(&t, 4096)?;
    (d.w == d.h).then_some((d.px, d.w))
}

/// Export : tuiles repeintes dans `stream`. Rend le nombre de tuiles écrites (paires minimap / minimap_sea).
pub fn export(w: &crate::gta::world::GameWorld, t: &crate::terrain::Terrain, stream: &std::path::Path, sea_all: bool) -> Result<usize, String> {
    let (mn, mx) = t.bounds();
    let tiles: Vec<(usize, usize)> = if sea_all { (0..ROWS).flat_map(|r| (0..COLS).map(move |c| (r, c))).collect() } else { tiles_for(mn, mx) };
    let mut n = 0;
    for (r, c) in tiles {
        for sea in [false, true] {
            let name = if sea { format!("minimap_sea_{r}_{c}") } else { format!("minimap_{r}_{c}") };
            let Some((mut px, size)) = game_tile(w, &name) else { continue };
            // mer au large : coin de la tuile 1_0 côté océan (bord ouest, au milieu), sinon une teinte du jeu
            let deep = {
                let k = (size / 2 * size + 2) * 4;
                if px.len() > k + 3 && !sea {
                    [px[k], px[k + 1], px[k + 2], 255]
                } else {
                    [64, 80, 86, 255]
                }
            };
            paint_tile(&mut px, size, r, c, t, sea, deep, sea_all);
            write_tile_ytd(stream, &name, size, px)?;
        }
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;

    #[test]
    fn grille_des_tuiles() {
        // Legion Square (195, -934) : rangée 1 (de 3900 à -600 ? non : -600 à -5100), colonne 0
        assert_eq!(tiles_for(v3(190.0, -940.0, 0.0), v3(200.0, -930.0, 0.0)), vec![(2, 0)]);
        let (x0, y0, x1, y1) = tile_rect(1, 0);
        assert_eq!((x0, y0, x1, y1), (-4140.0, -600.0, 360.0, 3900.0));
        // un terrain plat au-dessus de la mer : la terre est peinte en gris foncé au bon endroit
        let mut t = crate::terrain::Terrain::new(v3(0.0, 2000.0, 0.0), 512.0, 2.0, 0);
        t.h = std::sync::Arc::new(vec![2.0; t.nx * t.ny]);
        let n = 512;
        let mut px = vec![200u8; n * n * 4];
        paint_tile(&mut px, n, 1, 0, &t, false, [60, 70, 80, 255], false);
        let (i, j) = (((0.0 - START_X) / (TILE / n as f32)) as usize, ((3900.0 - 2000.0) / (TILE / n as f32)) as usize);
        let k = (j * n + i) * 4;
        assert_eq!(&px[k..k + 3], &LAND);
        assert_eq!(px[0], 200);
    }
}
