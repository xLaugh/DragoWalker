//! Décodage des textures compressées du jeu (DXT1, DXT3, DXT5) en RGBA, pour les aperçus dessinés sans la
//! carte graphique (vignettes et aperçu de la bibliothèque).

use crate::gta::drawable::{TexData, TexFmt};

/// Image RGBA (8 bits par composante).
#[derive(Clone)]
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Rgba {
    /// Couleur au point (u, v) de la texture, en répétant la texture hors de [0, 1].
    pub fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        let fu = u - u.floor();
        let fv = v - v.floor();
        let x = ((fu * self.w as f32) as usize).min(self.w - 1);
        let y = ((fv * self.h as f32) as usize).min(self.h - 1);
        let i = (y * self.w + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
}

fn c565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 255 + 15) / 31) as u8, ((g * 255 + 31) / 63) as u8, ((b * 255 + 15) / 31) as u8]
}

/// Bloc de couleur DXT (8 octets) -> 16 couleurs ; `dxt1` : bloc DXT1 (transparence possible).
fn color_block(b: &[u8], dxt1: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (a, bb) = (c565(c0), c565(c1));
    let mix = |p: u32, q: u32, d: u32| -> [u8; 4] {
        [
            ((a[0] as u32 * p + bb[0] as u32 * q) / d) as u8,
            ((a[1] as u32 * p + bb[1] as u32 * q) / d) as u8,
            ((a[2] as u32 * p + bb[2] as u32 * q) / d) as u8,
            255,
        ]
    };
    let pal: [[u8; 4]; 4] = if c0 > c1 || !dxt1 {
        [[a[0], a[1], a[2], 255], [bb[0], bb[1], bb[2], 255], mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [[a[0], a[1], a[2], 255], [bb[0], bb[1], bb[2], 255], mix(1, 1, 2), [0, 0, 0, 0]]
    };
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal[((idx >> (i * 2)) & 3) as usize];
    }
    out
}

/// Décode un niveau de texture en RGBA. BC7 n'est pas décodé (gris moyen).
pub fn decode(fmt: TexFmt, w: usize, h: usize, data: &[u8]) -> Option<Rgba> {
    if w == 0 || h == 0 {
        return None;
    }
    let mut px = vec![0u8; w * h * 4];
    match fmt {
        TexFmt::Rgba8 => {
            if data.len() < w * h * 4 {
                return None;
            }
            px.copy_from_slice(&data[..w * h * 4]);
        }
        TexFmt::Bc7 => {
            for p in px.chunks_mut(4) {
                p.copy_from_slice(&[150, 150, 150, 255]);
            }
        }
        TexFmt::Dxt1 | TexFmt::Dxt3 | TexFmt::Dxt5 => {
            let bsize = if fmt == TexFmt::Dxt1 { 8 } else { 16 };
            let bw = w.div_ceil(4);
            let bh = h.div_ceil(4);
            if data.len() < bw * bh * bsize {
                return None;
            }
            for by in 0..bh {
                for bx in 0..bw {
                    let b = &data[(by * bw + bx) * bsize..][..bsize];
                    let (alpha, cb) = match fmt {
                        TexFmt::Dxt1 => (None, &b[..8]),
                        TexFmt::Dxt3 => {
                            let mut a = [0u8; 16];
                            for (i, v) in a.iter_mut().enumerate() {
                                let nib = (b[i / 2] >> ((i % 2) * 4)) & 15;
                                *v = nib * 17;
                            }
                            (Some(a), &b[8..16])
                        }
                        _ => {
                            let (a0, a1) = (b[0] as u32, b[1] as u32);
                            let mut bits = 0u64;
                            for k in 0..6 {
                                bits |= (b[2 + k] as u64) << (8 * k);
                            }
                            let mut a = [0u8; 16];
                            for (i, v) in a.iter_mut().enumerate() {
                                let code = ((bits >> (3 * i)) & 7) as u32;
                                *v = match code {
                                    0 => a0,
                                    1 => a1,
                                    c if a0 > a1 => ((8 - c) * a0 + (c - 1) * a1) / 7,
                                    6 => 0,
                                    7 => 255,
                                    c => ((6 - c) * a0 + (c - 1) * a1) / 5,
                                } as u8;
                            }
                            (Some(a), &b[8..16])
                        }
                    };
                    let cols = color_block(cb, fmt == TexFmt::Dxt1);
                    for i in 0..16 {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x >= w || y >= h {
                            continue;
                        }
                        let o = (y * w + x) * 4;
                        let mut c = cols[i];
                        if let Some(a) = alpha {
                            c[3] = a[i];
                        }
                        px[o..o + 4].copy_from_slice(&c);
                    }
                }
            }
        }
    }
    Some(Rgba { w, h, px })
}

/// Le plus petit niveau de la texture dont le côté atteint `want` pixels (ou le plus grand s'il n'y en a pas), décodé.
pub fn decode_level(t: &TexData, want: u32) -> Option<Rgba> {
    let mut best = 0usize;
    for (i, _) in t.levels.iter().enumerate() {
        let lw = (t.w >> i).max(1);
        let lh = (t.h >> i).max(1);
        if lw.max(lh) >= want {
            best = i;
        }
    }
    let lw = (t.w >> best).max(1) as usize;
    let lh = (t.h >> best).max(1) as usize;
    decode(t.fmt, lw, lh, t.levels.get(best)?)
}

/// Taille d'un niveau de texture « simple » (codes D3DFMT du jeu), en octets.
pub fn raw_size(code: u32, w: usize, h: usize) -> usize {
    let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
    match code {
        25 => w * h * 2,
        28 | 50 => w * h,
        826889281 => blocks * 8,  // ATI1 / BC4
        843666497 => blocks * 16, // ATI2 / BC5
        _ => w * h * 4,           // 21, 22, 32
    }
}

/// Bloc à 8 niveaux interpolés (même codage que l'alpha DXT5) -> 16 valeurs.
fn interp_block(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= (b[2 + k] as u64) << (8 * k);
    }
    let mut out = [0u8; 16];
    for (i, v) in out.iter_mut().enumerate() {
        let c = ((bits >> (3 * i)) & 7) as u32;
        *v = match c {
            0 => a0,
            1 => a1,
            c if a0 > a1 => ((8 - c) * a0 + (c - 1) * a1) / 7,
            6 => 0,
            7 => 255,
            c => ((6 - c) * a0 + (c - 1) * a1) / 5,
        } as u8;
    }
    out
}

/// Convertit un niveau « simple » en RGBA, comme les conversions de DDSIO (CodeWalker) :
/// 21 A8R8G8B8 (en mémoire B, G, R, A) ; 22 X8R8G8B8 (alpha 255) ; 32 A8B8G8R8 (déjà R, G, B, A) ;
/// 25 A1R5G5B5 (ConvertBGR5A1ToRGBA8 0x06000525) ; 28 A8 -> (a, a, a, 255) (ConvertA8ToRGBA8 0x06000523) ;
/// 50 L8 -> gris (CodeWalker : rouge seul, ConvertR8ToRGBA8 0x06000524) ; BC4 -> gris (CodeWalker : rouge seul,
/// DecompressBC4Block 0x0600051f) ; BC5 -> (r, g, 0, 255) (DecompressBC5Block 0x06000522).
pub fn raw_to_rgba(code: u32, w: usize, h: usize, d: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    match code {
        21 | 22 => {
            for (o, p) in out.chunks_exact_mut(4).zip(d.chunks_exact(4)) {
                o.copy_from_slice(&[p[2], p[1], p[0], if code == 22 { 255 } else { p[3] }]);
            }
        }
        32 => {
            let n = out.len().min(d.len());
            out[..n].copy_from_slice(&d[..n]);
        }
        25 => {
            for (o, p) in out.chunks_exact_mut(4).zip(d.chunks_exact(2)) {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let ex = |c: u16| ((c as u32 * 255 + 15) / 31) as u8;
                o.copy_from_slice(&[ex((v >> 10) & 31), ex((v >> 5) & 31), ex(v & 31), if v >> 15 != 0 { 255 } else { 0 }]);
            }
        }
        28 | 50 => {
            for (o, v) in out.chunks_exact_mut(4).zip(d.iter()) {
                o.copy_from_slice(&[*v, *v, *v, 255]);
            }
        }
        826889281 | 843666497 => {
            let bw = w.div_ceil(4).max(1);
            let bh = h.div_ceil(4).max(1);
            let bs = if code == 826889281 { 8 } else { 16 };
            for by in 0..bh {
                for bx in 0..bw {
                    let off = (by * bw + bx) * bs;
                    if off + bs > d.len() {
                        continue;
                    }
                    let r = interp_block(&d[off..off + 8]);
                    let g = if bs == 16 { Some(interp_block(&d[off + 8..off + 16])) } else { None };
                    for i in 0..16 {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x >= w || y >= h {
                            continue;
                        }
                        let o = (y * w + x) * 4;
                        let px = match g {
                            Some(g) => [r[i], g[i], 0, 255],
                            None => [r[i], r[i], r[i], 255],
                        };
                        out[o..o + 4].copy_from_slice(&px);
                    }
                }
            }
        }
        _ => {}
    }
    out
}
