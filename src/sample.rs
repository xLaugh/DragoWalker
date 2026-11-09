//! Carte d'exemple : une petite ville générée qui remplace la vraie carte du jeu
//! (DragoWalker ne lit pas encore les archives RPF). Tout y est modifiable / supprimable.

use crate::math::{v3, Quat, Rng, V3};
use crate::model::{Entity, Ymap};

pub const BLOCK: f32 = 120.0;
pub const ROAD: f32 = 20.0;
pub const GRID: i32 = 4; // blocs de -GRID à GRID-1
pub const CITY_HALF: f32 = BLOCK * GRID as f32;
pub const BEACH: f32 = 80.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Buildings,
    Park,
    Parking,
}

pub fn block_kind(bx: i32, by: i32) -> BlockKind {
    match (bx, by) {
        (0, 0) | (-1, 0) => BlockKind::Park,
        (2, -2) | (-3, 1) => BlockKind::Parking,
        (-2, -3) => BlockKind::Park,
        _ => BlockKind::Buildings,
    }
}

/// Coin bas-gauche (x, y) de l'intérieur d'un bloc (hors route).
pub fn block_origin(bx: i32, by: i32) -> (f32, f32) {
    (bx as f32 * BLOCK + ROAD * 0.5, by as f32 * BLOCK + ROAD * 0.5)
}

pub fn generate() -> Ymap {
    let mut y = Ymap::new("exemple_ville");
    y.sample = true;
    let mut rng = Rng::new(48);
    let inner = BLOCK - ROAD;
    for by in -GRID..GRID {
        for bx in -GRID..GRID {
            let (ox, oy) = block_origin(bx, by);
            let center_dist = ((bx as f32 + 0.5).powi(2) + (by as f32 + 0.5).powi(2)).sqrt();
            match block_kind(bx, by) {
                BlockKind::Buildings => {
                    // 2x2 parcelles par bloc, hauteur plus grande au centre
                    let lots = if rng.f() < 0.35 { 1 } else { 2 };
                    let lot = inner / lots as f32;
                    for ly in 0..lots {
                        for lx in 0..lots {
                            let w = (lot - rng.range(8.0, 16.0)).max(12.0);
                            let d = (lot - rng.range(8.0, 16.0)).max(12.0);
                            let base_h = (70.0 - center_dist * 13.0).max(10.0);
                            let h = (base_h * rng.range(0.5, 1.4)).round().max(8.0);
                            let name = format!("dw_batiment_{}x{}x{}", w.round(), d.round(), h);
                            let cx = ox + lot * (lx as f32 + 0.5);
                            let cy = oy + lot * (ly as f32 + 0.5);
                            let mut e = Entity::new(&name, v3(cx, cy, 0.0));
                            e.lod_dist = 500.0;
                            y.entities.push(e);
                        }
                    }
                    // lampadaires aux coins du trottoir
                    for (cx, cy) in [(ox + 3.0, oy + 3.0), (ox + inner - 3.0, oy + 3.0), (ox + 3.0, oy + inner - 3.0), (ox + inner - 3.0, oy + inner - 3.0)] {
                        y.entities.push(Entity::new("prop_streetlight_01", v3(cx, cy, 0.0)));
                    }
                    // quelques poubelles / bancs
                    for _ in 0..3 {
                        let side = rng.next_u32() % 4;
                        let t = rng.range(10.0, inner - 10.0);
                        let (px, py, ang) = match side {
                            0 => (ox + t, oy + 2.0, 0.0),
                            1 => (ox + t, oy + inner - 2.0, 180.0f32),
                            2 => (ox + 2.0, oy + t, 270.0),
                            _ => (ox + inner - 2.0, oy + t, 90.0),
                        };
                        let arch = if rng.f() < 0.5 { "prop_bench_01a" } else { "prop_bin_01a" };
                        let mut e = Entity::new(arch, v3(px, py, 0.0));
                        e.rot = Quat::from_z(ang.to_radians());
                        y.entities.push(e);
                    }
                }
                BlockKind::Park => {
                    for _ in 0..26 {
                        let p = v3(ox + rng.range(6.0, inner - 6.0), oy + rng.range(6.0, inner - 6.0), 0.0);
                        let arch = match rng.next_u32() % 4 {
                            0 => "prop_tree_pine_01",
                            1 => "prop_bush_lrg_04b",
                            _ => "prop_tree_oak_01",
                        };
                        let mut e = Entity::new(arch, p);
                        e.rot = Quat::from_z(rng.range(0.0, 6.28));
                        y.entities.push(e);
                    }
                    for i in 0..4 {
                        let mut e = Entity::new("prop_bench_01a", v3(ox + inner * 0.5 + (i as f32 - 1.5) * 8.0, oy + inner * 0.5, 0.0));
                        e.rot = Quat::from_z(0.0);
                        y.entities.push(e);
                    }
                }
                BlockKind::Parking => {
                    for i in 0..6 {
                        y.entities.push(Entity::new("prop_mp_cone_01", v3(ox + 10.0 + i as f32 * 15.0, oy + inner * 0.5, 0.0)));
                    }
                    y.entities.push(Entity::new("prop_container_01a", v3(ox + 20.0, oy + 15.0, 0.0)));
                    y.entities.push(Entity::new("prop_dumpster_01a", v3(ox + inner - 10.0, oy + 8.0, 0.0)));
                }
            }
        }
    }
    // palmiers sur la plage
    let edge = CITY_HALF + BEACH * 0.5;
    let mut t = -CITY_HALF;
    while t < CITY_HALF {
        for (x, yy) in [(t, -edge), (t, edge), (-edge, t), (edge, t)] {
            if rng.f() < 0.6 {
                let mut e = Entity::new("prop_palm_med_01b", v3(x + rng.range(-8.0, 8.0), yy + rng.range(-8.0, 8.0), 0.0));
                e.rot = Quat::from_z(rng.range(0.0, 6.28));
                y.entities.push(e);
            }
        }
        t += 45.0;
    }
    y
}

/// Le sol à dessiner : (polygone au sol, couleur).
pub fn ground() -> Vec<([V3; 4], [u8; 3])> {
    let mut out = Vec::new();
    let quad = |x0: f32, y0: f32, x1: f32, y1: f32| [v3(x0, y0, 0.0), v3(x1, y0, 0.0), v3(x1, y1, 0.0), v3(x0, y1, 0.0)];
    let w = CITY_HALF + BEACH + 3000.0;
    // mer
    out.push((quad(-w, -w, w, w), [38, 84, 120]));
    // plage
    let b = CITY_HALF + BEACH;
    out.push((quad(-b, -b, b, b), [205, 190, 145]));
    // asphalte (routes)
    let c = CITY_HALF + ROAD * 0.5;
    out.push((quad(-c, -c, c, c), [72, 74, 78]));
    // blocs
    for by in -GRID..GRID {
        for bx in -GRID..GRID {
            let (ox, oy) = block_origin(bx, by);
            let inner = BLOCK - ROAD;
            let col = match block_kind(bx, by) {
                BlockKind::Buildings => [150, 148, 140],
                BlockKind::Park => [86, 140, 70],
                BlockKind::Parking => [100, 100, 104],
            };
            out.push((quad(ox, oy, ox + inner, oy + inner), col));
        }
    }
    out
}
