//! Maillage de déplacement des piétons (navmesh) du terrain du projet : fichiers navmesh[X][Y].ynv du jeu.
//!
//! Format (relu sur les navmesh du jeu avec CodeWalker, YnvFile / YnvPoly / NavMeshPoly) :
//! - une tuile de 150 m ; X = (x_min + 6000) / 50 et Y = (y_min + 6000) / 50, multiples de 3 ; AreaID =
//!   (Y / 3) × 100 + X / 3 (navmesh[102][102] : AreaID 3434, de -900 à -750) ;
//! - polygones convexes, sommets dans le sens inverse des aiguilles d'une montre ; le bord i va du sommet i au
//!   sommet i + 1 et donne le polygone voisin « AreaID:indice » (16383:16383 : aucun), voisin d'une autre tuile
//!   compris ;
//! - « Flags » = 6 octets : PolyFlags0 & 0xFF (4 = trottoir, 64 = pente raide), PolyFlags1 & 0xFF, PolyFlags1 >> 9
//!   (1 = sol plat, 2 = route, 32 = trottoir), PolyFlags2 >> 16 (pentes), puis PolyFlags2 & 0xFF et >> 8 & 0xFF :
//!   le centre du polygone (moyenne des sommets) placé dans sa boîte, de 0 à 255 (77 % des grands polygones du jeu
//!   à 1 près, quelques unités d'écart sur les petits : c'est le point de passage du polygone, pas une structure) ;
//! - CodeWalker (XmlYnv → YnvFile.BuildStructs) refait les secteurs et les boîtes à la conversion.
//!
//! Le terrain est découpé en cases de CELL m (alignées sur le monde, donc sur les tuiles) ; une case est retirée
//! sous l'eau, sur une pente trop raide, sous un bâtiment. Chaque case marche sur route (drapeau route), trottoir
//! (drapeau trottoir) ou sol.

use crate::math::{v3, V3};
use std::collections::HashMap;

pub const TILE: f32 = 150.0;
pub const ORIGIN: f32 = -6000.0;
/// Côté d'une case (m) — **estimé** (les polygones du jeu font 1 à 10 m ; 50 cases par tuile, 2 500 polygones au
/// plus, quand le jeu en a 2 000 à 4 000).
pub const CELL: f32 = 3.0;
/// Pente maximale d'une case (dénivelé / longueur) — **estimé** (39°).
pub const MAX_SLOPE: f32 = 0.8;
/// Pente sous laquelle une case est « sol plat » — **estimé**.
pub const FLAT: f32 = 0.06;
/// Hauteur sous laquelle le sol est sous l'eau de la mer (m).
pub const SEA: f32 = 0.2;
pub const NONE: u32 = 16383;

/// Genre d'une case.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ground {
    Soil,
    Sidewalk,
    Road,
}

/// Tuile qui contient x (indice de tuile, pas le X du nom de fichier).
pub fn tile_of(x: f32) -> i32 {
    ((x - ORIGIN) / TILE).floor() as i32
}

pub fn file_name(tx: i32, ty: i32) -> String {
    format!("navmesh[{}][{}]", tx * 3, ty * 3)
}

pub fn area_id(tx: i32, ty: i32) -> u32 {
    (ty * 100 + tx) as u32
}

/// Une case gardée : coin bas-gauche (indices de case du monde), hauteurs des 4 coins, genre.
#[derive(Clone, Debug)]
pub struct Cell {
    pub ci: i32,
    pub cj: i32,
    pub z: [f32; 4],
    pub ground: Ground,
}

/// Cases marchables d'un rectangle du monde. `h` : hauteur du sol (None : hors terrain) ; `blocked` : point sous un
/// bâtiment ou dans l'eau ; `ground` : genre du sol au centre d'une case.
pub fn cells(mn: [f32; 2], mx: [f32; 2], h: &dyn Fn(f32, f32) -> Option<f32>, blocked: &dyn Fn(f32, f32, f32) -> bool, ground: &dyn Fn(f32, f32) -> Ground) -> Vec<Cell> {
    let (i0, i1) = ((mn[0] / CELL).ceil() as i32, (mx[0] / CELL).floor() as i32);
    let (j0, j1) = ((mn[1] / CELL).ceil() as i32, (mx[1] / CELL).floor() as i32);
    let mut out = Vec::new();
    for cj in j0..j1 {
        for ci in i0..i1 {
            let (x0, y0) = (ci as f32 * CELL, cj as f32 * CELL);
            let corners = [(x0, y0), (x0 + CELL, y0), (x0 + CELL, y0 + CELL), (x0, y0 + CELL)];
            let mut z = [0.0f32; 4];
            let mut ok = true;
            for (k, c) in corners.iter().enumerate() {
                match h(c.0, c.1) {
                    Some(v) => z[k] = v,
                    None => ok = false,
                }
            }
            if !ok {
                continue;
            }
            let (lo, hi) = z.iter().fold((f32::MAX, f32::MIN), |r, v| (r.0.min(*v), r.1.max(*v)));
            if hi - lo > MAX_SLOPE * CELL * std::f32::consts::SQRT_2 {
                continue;
            }
            let (cx, cy) = (x0 + CELL * 0.5, y0 + CELL * 0.5);
            let zc = (z[0] + z[1] + z[2] + z[3]) * 0.25;
            if blocked(cx, cy, zc) || corners.iter().zip(z.iter()).any(|(c, zz)| blocked(c.0, c.1, *zz)) {
                continue;
            }
            out.push(Cell { ci, cj, z, ground: ground(cx, cy) });
        }
    }
    out
}

/// XML (CodeWalker) de chaque tuile : (nom de fichier sans extension, XML, nombre de polygones).
pub fn tiles_xml(cells: &[Cell]) -> Vec<(String, String, usize)> {
    // indice de chaque case dans sa tuile
    let per = (TILE / CELL).round() as i32;
    let tile = |ci: i32| tile_of(ci as f32 * CELL + CELL * 0.5);
    let mut by_tile: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, c) in cells.iter().enumerate() {
        by_tile.entry((tile(c.ci), tile(c.cj))).or_default().push(k);
    }
    let mut index: HashMap<(i32, i32), (u32, u32)> = HashMap::new();
    for (&(tx, ty), list) in &by_tile {
        for (n, &k) in list.iter().enumerate() {
            index.insert((cells[k].ci, cells[k].cj), (area_id(tx, ty), n as u32));
        }
    }
    let _ = per;
    let mut out = Vec::new();
    let mut keys: Vec<(i32, i32)> = by_tile.keys().copied().collect();
    keys.sort();
    for (tx, ty) in keys {
        let list = &by_tile[&(tx, ty)];
        if list.len() as u32 >= NONE {
            continue;
        }
        let (bx, by) = (ORIGIN + tx as f32 * TILE, ORIGIN + ty as f32 * TILE);
        let (mut zmin, mut zmax) = (f32::MAX, f32::MIN);
        for &k in list {
            for z in cells[k].z {
                zmin = zmin.min(z);
                zmax = zmax.max(z);
            }
        }
        let mut s = String::with_capacity(list.len() * 400);
        s += "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<NavMesh>\n <ContentFlags>Polygons</ContentFlags>\n";
        s += &format!(" <AreaID value=\"{}\" />\n", area_id(tx, ty));
        s += &format!(" <BBMin x=\"{bx}\" y=\"{by}\" z=\"{zmin}\" />\n <BBMax x=\"{}\" y=\"{}\" z=\"{zmax}\" />\n <BBSize x=\"{TILE}\" y=\"{TILE}\" z=\"{}\" />\n", bx + TILE, by + TILE, (zmax - zmin).max(0.01));
        s += " <Polygons>\n";
        for &k in list {
            let c = &cells[k];
            let (x0, y0) = (c.ci as f32 * CELL, c.cj as f32 * CELL);
            let slope = {
                let (lo, hi) = c.z.iter().fold((f32::MAX, f32::MIN), |r, v| (r.0.min(*v), r.1.max(*v)));
                (hi - lo) / CELL
            };
            // octets : PolyFlags0, PolyFlags1, PolyFlags1 >> 9, pentes, centre x, centre y (milieu d'un carré)
            let (f1, f3) = match c.ground {
                Ground::Road => (0, 2),
                Ground::Sidewalk => (4, 33),
                Ground::Soil => (0, if slope < FLAT { 1 } else { 0 }),
            };
            s += &format!("  <Item>\n   <Flags>{f1} 0 {f3} 0 127 127</Flags>\n   <Vertices>\n");
            let pts = [(x0, y0, c.z[0]), (x0 + CELL, y0, c.z[1]), (x0 + CELL, y0 + CELL, c.z[2]), (x0, y0 + CELL, c.z[3])];
            for p in pts {
                s += &format!("    {:.4}, {:.4}, {:.4}\n", p.0, p.1, p.2);
            }
            s += "   </Vertices>\n   <Edges>\n";
            // bord i : du sommet i au sommet i + 1 → voisin sud, est, nord, ouest
            for (di, dj) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                match index.get(&(c.ci + di, c.cj + dj)) {
                    Some((a, p)) => s += &format!("    {a}:{p}, {a}:{p}\n"),
                    None => s += &format!("    {NONE}:{NONE}, {NONE}:{NONE}\n"),
                }
            }
            s += "   </Edges>\n  </Item>\n";
        }
        s += " </Polygons>\n</NavMesh>\n";
        out.push((file_name(tx, ty), s, list.len()));
    }
    out
}

impl crate::app::DwApp {
    /// Retire les navmesh d'un export précédent.
    pub fn export_navmesh_clear(&self, stream: &std::path::Path) -> std::io::Result<()> {
        for f in std::fs::read_dir(stream)?.flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if n.starts_with("navmesh[") && (n.ends_with(".ynv") || n.ends_with(".ynv.xml")) {
                std::fs::remove_file(f.path())?;
            }
        }
        Ok(())
    }

    /// Navmesh du terrain du projet dans `stream` (XML à convertir par CodeWalker). Rend (tuiles, polygones).
    pub fn export_navmesh(&self, stream: &std::path::Path) -> Result<(usize, usize), String> {
        let _ = self.export_navmesh_clear(stream);
        let Some(t) = self.proj.terrain.as_ref() else { return Ok((0, 0)) };
        let (mn, mx) = t.bounds();
        // bâtiments : atelier (emprise exacte) et gros objets du projet (boîte, rentrée de 30 cm)
        let mut boxes: Vec<(V3, crate::math::Quat, V3, V3)> = Vec::new();
        let mut houses: Vec<(V3, crate::math::Quat, Vec<[f32; 2]>)> = Vec::new();
        for y in self.proj.ymaps.iter().filter(|y| !y.sample) {
            for e in &y.entities {
                if e.mlo.is_some() || crate::vegetation::is_vegetation(&e.archetype) {
                    continue;
                }
                if let Some(h) = self.proj.houses.iter().find(|h| h.name.eq_ignore_ascii_case(&e.archetype)) {
                    houses.push((e.pos, e.rot, h.foot.clone()));
                    continue;
                }
                let (a, b) = e.local_box(&self.proj);
                let s = b - a;
                if s.x.max(s.y) * e.scale_xy >= 4.0 && s.z * e.scale_z >= 2.0 {
                    let sc = v3(e.scale_xy, e.scale_xy, e.scale_z);
                    boxes.push((e.pos, e.rot, a.mulv(sc) + v3(0.3, 0.3, 0.0), b.mulv(sc) - v3(0.3, 0.3, 0.0)));
                }
            }
        }
        let blocked = |x: f32, y: f32, z: f32| -> bool {
            if z < SEA || t.waters.iter().any(|w| x >= w.min[0] && x <= w.max[0] && y >= w.min[1] && y <= w.max[1] && z < w.z - 0.1) {
                return true;
            }
            let p = v3(x, y, z);
            for (pos, rot, foot) in &houses {
                let l = rot.inverse().rotate(p - *pos);
                if crate::atelier::point_in_poly([l.x, l.y], foot) {
                    return true;
                }
            }
            for (pos, rot, a, b) in &boxes {
                let l = rot.inverse().rotate(p - *pos);
                if l.x > a.x && l.x < b.x && l.y > a.y && l.y < b.y && l.z > a.z - 1.0 && l.z < b.z {
                    return true;
                }
            }
            false
        };
        let ground = |x: f32, y: f32| -> Ground {
            let mut g = Ground::Soil;
            for r in &t.roads {
                if r.elevated {
                    continue;
                }
                if let Some((d, _)) = r.nearest(x, y) {
                    if d < r.kind().width * 0.5 {
                        return Ground::Road;
                    }
                    if d < r.half_total() {
                        g = Ground::Sidewalk;
                    }
                }
            }
            g
        };
        let cs = cells([mn.x, mn.y], [mx.x, mx.y], &|x, y| t.height_at(x, y), &blocked, &ground);
        let tiles = tiles_xml(&cs);
        let mut np = 0;
        for (name, xml, n) in &tiles {
            std::fs::write(stream.join(format!("{name}.ynv.xml")), xml).map_err(|e| e.to_string())?;
            np += n;
        }
        Ok((tiles.len(), np))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_et_zone_des_tuiles() {
        // navmesh[102][102] du jeu : AreaID 3434, de -900 à -750
        assert_eq!(tile_of(-900.0), 34);
        assert_eq!(tile_of(-750.1), 34);
        assert_eq!(file_name(34, 34), "navmesh[102][102]");
        assert_eq!(area_id(34, 34), 3434);
    }

    #[test]
    fn cases_voisines_et_tuiles_reliees() {
        // terrain plat de -760 à -740 (à cheval sur deux tuiles), un trou au milieu (bâtiment)
        let cs = cells([-760.0, 0.0], [-740.0, 9.0], &|_, _| Some(5.0), &|x, y, _| (x + 751.5).abs() < 1.0 && (y - 4.5).abs() < 1.0, &|_, _| Ground::Soil);
        assert!(!cs.is_empty());
        assert!(!cs.iter().any(|c| c.ci == -251 && c.cj == 1), "case du bâtiment retirée");
        let tiles = tiles_xml(&cs);
        assert_eq!(tiles.len(), 2, "deux tuiles");
        // la case juste à gauche du bord (-750) a pour voisin est une case de l'autre tuile
        let left = tiles.iter().find(|t| t.0 == "navmesh[102][120]").unwrap();
        let right_area = area_id(35, 40);
        assert!(left.1.contains(&format!("{right_area}:")), "lien vers la tuile voisine");
        assert!(left.1.contains("<Flags>0 0 1 0 127 127</Flags>"));
        assert!(left.1.contains("16383:16383"));
    }

    /// Conversion réelle (DW_CW_DIR) : `cargo test -- --ignored navmesh_cw --nocapture`.
    #[test]
    #[ignore]
    fn navmesh_cw() {
        let dll = crate::cwbridge::dll(&std::env::var("DW_CW_DIR").unwrap_or_default()).expect("CodeWalker");
        let dir = std::env::temp_dir().join("dw_test_navmesh_cw");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cs = cells([-800.0, -100.0], [-700.0, -40.0], &|x, y| Some(5.0 + (x * 0.05).sin() + (y * 0.03).cos()), &|_, _, _| false, &|x, _| if (x + 760.0).abs() < 5.0 { Ground::Road } else { Ground::Soil });
        let tiles = tiles_xml(&cs);
        for (n, x, _) in &tiles {
            std::fs::write(dir.join(format!("{n}.ynv.xml")), x).unwrap();
        }
        let (n, errs) = crate::cwbridge::to_binary(&dll, &dir).unwrap();
        println!("convertis {n}, erreurs {errs:?}");
        assert!(errs.is_empty());
        let back = dir.join("back");
        std::fs::create_dir_all(&back).unwrap();
        let (name, xml, np) = &tiles[0];
        crate::cwbridge::to_xml(&dll, &[(dir.join(format!("{name}.ynv")), back.join("a.xml"))]).unwrap();
        let a = std::fs::read_to_string(back.join("a.xml")).unwrap();
        println!("{} : {} polygones, binaire {} octets", name, np, std::fs::metadata(dir.join(format!("{name}.ynv"))).unwrap().len());
        println!("{}", &a[..a.len().min(900)]);
        assert_eq!(a.matches("<Flags>").count(), *np);
        assert!(a.contains("127 127"));
        let _ = xml;
    }
}
