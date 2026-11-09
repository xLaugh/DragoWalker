//! Carte des hauteurs du jeu (heightmap.dat), lue comme HeightmapFile::Read (0x06000dbf) :
//! en-tête « HMAP », grille Largeur × Hauteur, lignes compressées (Start, Count, DataOffset),
//! hauteurs max puis min sur 1 octet entre BBMin.z et BBMax.z.

use crate::math::{v3, V3};

pub struct Heightmap {
    pub w: usize,
    pub h: usize,
    pub min: V3,
    pub max: V3,
    pub max_h: Vec<u8>,
    pub min_h: Vec<u8>,
}

const HMAP_MAGIC: u32 = 1213022544; // valeur de HeightmapFile::.ctor (0x06000dbb)

impl Heightmap {
    pub fn parse(d: &[u8]) -> Option<Heightmap> {
        if d.len() < 44 {
            return None;
        }
        let le = u32::from_le_bytes(d[0..4].try_into().ok()?) == HMAP_MAGIC;
        let u16_at = |p: usize| -> u16 {
            let b = [d[p], d[p + 1]];
            if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }
        };
        let u32_at = |p: usize| -> u32 {
            let b: [u8; 4] = d[p..p + 4].try_into().unwrap();
            if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
        };
        let f32_at = |p: usize| f32::from_bits(u32_at(p));
        let compressed = u32_at(8);
        let w = u16_at(12) as usize;
        let h = u16_at(14) as usize;
        let min = v3(f32_at(16), f32_at(20), f32_at(24));
        let max = v3(f32_at(28), f32_at(32), f32_at(36));
        let mut length = u32_at(40) as usize;
        let mut p = 44;
        let mut max_h = vec![0u8; w * h];
        let mut min_h = vec![0u8; w * h];
        if compressed > 0 {
            let mut heads = Vec::with_capacity(h);
            for _ in 0..h {
                if p + 8 > d.len() {
                    return None;
                }
                heads.push((u16_at(p) as usize, u16_at(p + 2) as usize, u32_at(p + 4) as i32 as usize));
                p += 8;
            }
            length = length.saturating_sub(h * 8);
            let data = d.get(p..p.checked_add(length)?)?;
            let half = length / 2;
            for (y, &(start, count, off)) in heads.iter().enumerate() {
                for i in 0..count {
                    let x = start + i;
                    let src = off.wrapping_add(x);
                    if x < w && src < data.len() && src.saturating_add(half) < data.len() {
                        max_h[y * w + x] = data[src];
                        min_h[y * w + x] = data[src + half];
                    }
                }
            }
        } else {
            let data = d.get(p..p.checked_add(length)?)?;
            let n = w * h;
            if data.len() < 2 * n {
                return None;
            }
            max_h.copy_from_slice(&data[..n]);
            min_h.copy_from_slice(&data[n..2 * n]);
        }
        Some(Heightmap { w, h, min, max, max_h, min_h })
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min.x && x <= self.max.x && y >= self.min.y && y <= self.max.y
    }

    fn cell(&self, x: f32, y: f32) -> (f32, f32) {
        let fx = (x - self.min.x) / (self.max.x - self.min.x) * (self.w - 1) as f32;
        let fy = (y - self.min.y) / (self.max.y - self.min.y) * (self.h - 1) as f32;
        (fx, fy)
    }

    pub fn byte_to_z(&self, b: u8) -> f32 {
        self.min.z + (self.max.z - self.min.z) * b as f32 / 255.0
    }

    /// Hauteur (max) du sol, interpolée. `None` hors de la carte ou sur une case vide.
    pub fn height(&self, x: f32, y: f32) -> Option<f32> {
        if !self.contains(x, y) {
            return None;
        }
        let (fx, fy) = self.cell(x, y);
        let x0 = (fx.floor() as usize).min(self.w - 1);
        let y0 = (fy.floor() as usize).min(self.h - 1);
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let s = |xx: usize, yy: usize| self.max_h[yy * self.w + xx] as f32;
        let a = s(x0, y0) * (1.0 - tx) + s(x1, y0) * tx;
        let b = s(x0, y1) * (1.0 - tx) + s(x1, y1) * tx;
        let v = a * (1.0 - ty) + b * ty;
        if self.max_h[y0 * self.w + x0] == 0 && self.min_h[y0 * self.w + x0] == 0 {
            return None;
        }
        Some(self.min.z + (self.max.z - self.min.z) * v / 255.0)
    }

    /// Hauteur basse du sol (le haut des bâtiments n'y compte pas), interpolée.
    pub fn height_min(&self, x: f32, y: f32) -> Option<f32> {
        if !self.contains(x, y) {
            return None;
        }
        let (fx, fy) = self.cell(x, y);
        let x0 = (fx.floor() as usize).min(self.w - 1);
        let y0 = (fy.floor() as usize).min(self.h - 1);
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let s = |xx: usize, yy: usize| self.min_h[yy * self.w + xx] as f32;
        if self.max_h[y0 * self.w + x0] == 0 && self.min_h[y0 * self.w + x0] == 0 {
            return None;
        }
        let a = s(x0, y0) * (1.0 - tx) + s(x1, y0) * tx;
        let b = s(x0, y1) * (1.0 - tx) + s(x1, y1) * tx;
        Some(self.byte_to_z((a * (1.0 - ty) + b * ty).round().clamp(0.0, 255.0) as u8).max(self.min.z.min(0.0)))
    }

    /// Position (monde) du coin d'une case.
    pub fn cell_pos(&self, cx: usize, cy: usize) -> (f32, f32) {
        (
            self.min.x + (self.max.x - self.min.x) * cx as f32 / (self.w - 1) as f32,
            self.min.y + (self.max.y - self.min.y) * cy as f32 / (self.h - 1) as f32,
        )
    }
}
