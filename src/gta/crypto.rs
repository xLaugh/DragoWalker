//! Chiffrement des archives GTA V (voir MECHANICS.md, section « Archives du jeu »).
//! Règles relevées dans CodeWalker.Core.dll : GTACrypto (0x06005b87..0x06005b8e),
//! GTA5Hash::CalculateHash (0x06005bb8), JenkHash::GenHash (0x06005bc5..0x06005bc7).

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};

/// Clés du jeu, lues sur l'ordinateur de la personne (jamais incluses dans l'application).
pub struct Keys {
    pub aes: [u8; 32],
    /// 101 clés NG de 272 octets, vues comme 68 entiers.
    pub ng_keys: Vec<[u32; 68]>,
    /// 17 tours × 16 tables × 256 entiers.
    pub ng_tables: Vec<Vec<[u32; 256]>>,
    pub lut: [u8; 256],
}

/// Hachage « Jenkins one-at-a-time » utilisé partout dans le jeu (noms en minuscules).
pub fn joaat_bytes(data: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in data {
        h = h.wrapping_add(b as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// Hachage d'un nom : minuscules, comme les noms de fichiers et d'archétypes du jeu.
pub fn joaat(s: &str) -> u32 {
    joaat_bytes(s.to_ascii_lowercase().as_bytes())
}

impl Keys {
    /// GTA5Hash::CalculateHash : hachage du nom d'archive avec la table LUT.
    pub fn name_hash(&self, name: &str) -> u32 {
        let mut r: u32 = 0;
        for b in name.bytes() {
            let t = 1025u32.wrapping_mul((self.lut[b as usize] as u32).wrapping_add(r));
            r = (t >> 6) ^ t;
        }
        let r9 = r.wrapping_mul(9);
        32769u32.wrapping_mul((r9 >> 11) ^ r9)
    }

    /// GTACrypto::GetNGKey : (hachage + taille + 61) mod 101.
    pub fn ng_key(&self, name: &str, length: u32) -> &[u32; 68] {
        let idx = self.name_hash(name).wrapping_add(length).wrapping_add(61) % 101;
        &self.ng_keys[idx as usize]
    }

    /// GTACrypto::DecryptNG : blocs entiers de 16 octets, le reste n'est pas touché.
    pub fn decrypt_ng(&self, data: &mut [u8], name: &str, length: u32) {
        let key = *self.ng_key(name, length);
        for block in data.chunks_exact_mut(16) {
            self.decrypt_ng_block(block, &key);
        }
    }

    fn decrypt_ng_block(&self, block: &mut [u8], key: &[u32; 68]) {
        let sub = |i: usize| [key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]];
        let mut b = [0u8; 16];
        b.copy_from_slice(block);
        b = round_a(&b, sub(0), &self.ng_tables[0]);
        b = round_a(&b, sub(1), &self.ng_tables[1]);
        for k in 2..=15 {
            b = round_b(&b, sub(k), &self.ng_tables[k]);
        }
        b = round_a(&b, sub(16), &self.ng_tables[16]);
        block.copy_from_slice(&b);
    }

    /// GTACrypto::DecryptAESData : AES-256 ECB sans bourrage, blocs entiers seulement.
    pub fn decrypt_aes(&self, data: &mut [u8]) {
        aes_ecb_decrypt(&self.aes, data);
    }
}

pub fn aes_ecb_decrypt(key: &[u8; 32], data: &mut [u8]) {
    let cipher = aes::Aes256::new(GenericArray::from_slice(key));
    for block in data.chunks_exact_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
}

fn out(x: [u32; 4]) -> [u8; 16] {
    let mut o = [0u8; 16];
    for i in 0..4 {
        o[4 * i..4 * i + 4].copy_from_slice(&x[i].to_le_bytes());
    }
    o
}

fn round_a(d: &[u8; 16], k: [u32; 4], t: &[[u32; 256]]) -> [u8; 16] {
    let f = |a: usize, b: usize, c: usize, e: usize| t[a][d[a] as usize] ^ t[b][d[b] as usize] ^ t[c][d[c] as usize] ^ t[e][d[e] as usize];
    out([
        f(0, 1, 2, 3) ^ k[0],
        f(4, 5, 6, 7) ^ k[1],
        f(8, 9, 10, 11) ^ k[2],
        f(12, 13, 14, 15) ^ k[3],
    ])
}

fn round_b(d: &[u8; 16], k: [u32; 4], t: &[[u32; 256]]) -> [u8; 16] {
    let f = |a: usize, b: usize, c: usize, e: usize| t[a][d[a] as usize] ^ t[b][d[b] as usize] ^ t[c][d[c] as usize] ^ t[e][d[e] as usize];
    out([
        f(0, 7, 10, 13) ^ k[0],
        f(1, 4, 11, 14) ^ k[1],
        f(2, 5, 8, 15) ^ k[2],
        f(3, 6, 9, 12) ^ k[3],
    ])
}

/// Décompression « deflate » brute (RpfFile::DecompressBytes).
pub fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::with_capacity(data.len() * 3);
    let mut d = flate2::read::DeflateDecoder::new(data);
    d.read_to_end(&mut out).ok()?;
    Some(out)
}
