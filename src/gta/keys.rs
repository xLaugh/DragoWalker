//! Chargement des clés du jeu, comme CodeWalker (GTA5Keys::UseMagicData 0x06005b9b) :
//! 1. la clé AES est retrouvée dans GTA5.exe grâce à son empreinte SHA-1 (HashSearch, pas de 8 octets) ;
//! 2. le bloc « magic » de CodeWalker.Core.dll est démêlé (System.Random initialisé avec
//!    JenkHash(clé AES)), déchiffré en AES puis décompressé ;
//! 3. il contient les 101 clés NG, les tables NG, la LUT des noms et la clé audio.
//! Rien de tout cela n'est inclus dans DragoWalker : tout est lu sur l'ordinateur de la personne.

use super::crypto::{aes_ecb_decrypt, inflate, joaat_bytes, Keys};
use rayon::prelude::*;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

/// Empreinte SHA-1 de la clé AES (GTA5KeyHashes::PC_AES_KEY_HASH, CodeWalker.Core.dll 0x06005b9d).
const AES_KEY_SHA1: [u8; 20] = [
    0xA0, 0x79, 0x61, 0x28, 0xA7, 0x75, 0x72, 0x0A, 0xC2, 0x04, 0xD9, 0x81, 0x9F, 0x68, 0xC1, 0x72, 0xE3, 0x95, 0x2C, 0x6D,
];
const NG_KEYS_LEN: usize = 27472; // 101 × 272
const NG_TABLES_LEN: usize = 278528; // 17 × 16 × 256 × 4
const LUT_LEN: usize = 256;

/// Recherche de la clé AES dans l'exécutable : SHA-1 de 32 octets tous les 8 octets,
/// par blocs de 1 Mo (HashSearch::SearchHashes, 0x06005ba1 / 0x06005eba).
pub fn find_aes_key(exe: &[u8]) -> Option<[u8; 32]> {
    const BLOCK: usize = 1048576; // HashSearch::SearchHashes 0x06005ba1
    let blocks = exe.len() / BLOCK;
    (0..blocks).into_par_iter().find_map_any(|k| {
        let mut i = 0;
        while i < BLOCK / 8 {
            let pos = k * BLOCK + i * 8;
            if pos + 32 <= exe.len() {
                let cand = &exe[pos..pos + 32];
                if Sha1::digest(cand).as_slice() == AES_KEY_SHA1 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(cand);
                    return Some(key);
                }
            }
            i += 1;
        }
        None
    })
}

/// Générateur pseudo-aléatoire de .NET (System.Random avec graine), nécessaire pour démêler « magic ».
struct DotNetRandom {
    seed: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl DotNetRandom {
    fn new(seed: i32) -> Self {
        const MBIG: i32 = i32::MAX;
        const MSEED: i32 = 161803398;
        let mut sa = [0i32; 56];
        let sub = if seed == i32::MIN { i32::MAX } else { seed.abs() };
        let mut mj = MSEED - sub;
        sa[55] = mj;
        let mut mk = 1i32;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            sa[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = sa[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                sa[i] = sa[i].wrapping_sub(sa[1 + (i + 30) % 55]);
                if sa[i] < 0 {
                    sa[i] = sa[i].wrapping_add(MBIG);
                }
            }
        }
        DotNetRandom { seed: sa, inext: 0, inextp: 21 }
    }

    fn sample(&mut self) -> i32 {
        let mut a = self.inext + 1;
        if a >= 56 {
            a = 1;
        }
        let mut b = self.inextp + 1;
        if b >= 56 {
            b = 1;
        }
        let mut r = self.seed[a].wrapping_sub(self.seed[b]);
        if r == i32::MAX {
            r -= 1;
        }
        if r < 0 {
            r = r.wrapping_add(i32::MAX);
        }
        self.seed[a] = r;
        self.inext = a;
        self.inextp = b;
        r
    }

    fn next_bytes(&mut self, buf: &mut [u8]) {
        for b in buf.iter_mut() {
            *b = (self.sample() % 256) as u8;
        }
    }
}

fn read_7bit(d: &[u8], p: &mut usize) -> Option<usize> {
    let mut v: usize = 0;
    let mut shift = 0;
    loop {
        let b = *d.get(*p)?;
        *p += 1;
        v |= ((b & 0x7F) as usize) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

fn i32_at(d: &[u8], p: usize) -> Option<i32> {
    Some(i32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?))
}

/// Lit une ressource « tableau d'octets » nommée dans un fichier .resources .NET contenu dans une DLL.
pub fn dotnet_resource_bytes(dll: &[u8], wanted: &str) -> Option<Vec<u8>> {
    let magic = 0xBEEFCACEu32.to_le_bytes();
    let mut search = 0;
    while let Some(off) = dll[search..].windows(4).position(|w| w == magic) {
        let start = search + off;
        search = start + 4;
        if let Some(v) = parse_resources(&dll[start..], wanted) {
            return Some(v);
        }
    }
    None
}

fn parse_resources(d: &[u8], wanted: &str) -> Option<Vec<u8>> {
    let mut p = 4;
    let hdr_version = i32_at(d, p)?;
    let skip = i32_at(d, p + 4)?;
    p += 8;
    if hdr_version > 1 {
        p += skip as usize;
    } else {
        for _ in 0..2 {
            let l = read_7bit(d, &mut p)?;
            p += l;
        }
    }
    let _version = i32_at(d, p)?;
    let num_res = i32_at(d, p + 4)? as usize;
    let num_types = i32_at(d, p + 8)? as usize;
    p += 12;
    if num_res > 100_000 || num_types > 10_000 {
        return None;
    }
    for _ in 0..num_types {
        let l = read_7bit(d, &mut p)?;
        p += l;
    }
    while p & 7 != 0 {
        p += 1;
    }
    p += 4 * num_res; // hachages des noms
    let name_pos_start = p;
    p += 4 * num_res;
    let data_section = i32_at(d, p)? as usize;
    p += 4;
    let name_section = p;
    for i in 0..num_res {
        let np = i32_at(d, name_pos_start + 4 * i)? as usize;
        let mut q = name_section + np;
        let blen = read_7bit(d, &mut q)?;
        let raw = d.get(q..q + blen)?;
        let name: String = char::decode_utf16(raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])))
            .map(|c| c.unwrap_or('?'))
            .collect();
        q += blen;
        let data_off = i32_at(d, q)? as usize;
        if name == wanted {
            let mut r = data_section + data_off;
            let type_code = read_7bit(d, &mut r)?;
            if type_code != 0x20 {
                return None;
            }
            let len = i32_at(d, r)? as usize;
            return d.get(r + 4..r + 4 + len).map(|s| s.to_vec());
        }
    }
    None
}

/// Démêle et décode le bloc « magic » (UseMagicData).
pub fn decode_magic(magic: &[u8], aes: &[u8; 32]) -> Option<Keys> {
    let seed = joaat_bytes(aes) as i32;
    let mut rnd = DotNetRandom::new(seed);
    let n = magic.len();
    let mut r = vec![vec![0u8; n]; 4];
    for buf in r.iter_mut() {
        rnd.next_bytes(buf);
    }
    let mut data: Vec<u8> = (0..n)
        .map(|i| magic[i].wrapping_sub(r[0][i]).wrapping_sub(r[1][i]).wrapping_sub(r[2][i]).wrapping_sub(r[3][i]))
        .collect();
    aes_ecb_decrypt(aes, &mut data);
    let plain = inflate(&data)?;
    if plain.len() < NG_KEYS_LEN + NG_TABLES_LEN + LUT_LEN {
        return None;
    }
    let u32_at = |p: usize| u32::from_le_bytes(plain[p..p + 4].try_into().unwrap());
    let mut ng_keys = Vec::with_capacity(101);
    for k in 0..101 {
        let mut key = [0u32; 68];
        for (j, v) in key.iter_mut().enumerate() {
            *v = u32_at(k * 272 + j * 4);
        }
        ng_keys.push(key);
    }
    let mut ng_tables = Vec::with_capacity(17);
    let mut p = NG_KEYS_LEN;
    for _ in 0..17 {
        let mut round = Vec::with_capacity(16);
        for _ in 0..16 {
            let mut t = [0u32; 256];
            for v in t.iter_mut() {
                *v = u32_at(p);
                p += 4;
            }
            round.push(t);
        }
        ng_tables.push(round);
    }
    let mut lut = [0u8; 256];
    lut.copy_from_slice(&plain[p..p + LUT_LEN]);
    Some(Keys { aes: *aes, ng_keys, ng_tables, lut })
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA"))?;
    Some(PathBuf::from(base).join("DragoWalker").join("keys.bin"))
}

fn save_cache(k: &Keys) {
    let Some(p) = cache_path() else { return };
    let mut out = Vec::with_capacity(32 + NG_KEYS_LEN + NG_TABLES_LEN + LUT_LEN);
    out.extend_from_slice(&k.aes);
    for key in &k.ng_keys {
        for v in key {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for round in &k.ng_tables {
        for t in round {
            for v in t {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    out.extend_from_slice(&k.lut);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, out);
}

fn load_cache(exe: &[u8]) -> Option<Keys> {
    let d = std::fs::read(cache_path()?).ok()?;
    if d.len() != 32 + NG_KEYS_LEN + NG_TABLES_LEN + LUT_LEN {
        return None;
    }
    let mut aes = [0u8; 32];
    aes.copy_from_slice(&d[..32]);
    // la clé doit toujours se trouver dans cet exécutable (sinon le jeu a changé)
    if !exe.windows(32).step_by(8).any(|w| w == aes) && !exe.windows(32).any(|w| w == aes) {
        return None;
    }
    let u32_at = |p: usize| u32::from_le_bytes(d[p..p + 4].try_into().unwrap());
    let mut ng_keys = Vec::new();
    let mut p = 32;
    for _ in 0..101 {
        let mut key = [0u32; 68];
        for v in key.iter_mut() {
            *v = u32_at(p);
            p += 4;
        }
        ng_keys.push(key);
    }
    let mut ng_tables = Vec::new();
    for _ in 0..17 {
        let mut round = Vec::new();
        for _ in 0..16 {
            let mut t = [0u32; 256];
            for v in t.iter_mut() {
                *v = u32_at(p);
                p += 4;
            }
            round.push(t);
        }
        ng_tables.push(round);
    }
    let mut lut = [0u8; 256];
    lut.copy_from_slice(&d[p..p + 256]);
    Some(Keys { aes, ng_keys, ng_tables, lut })
}

/// Cherche CodeWalker.Core.dll : dossier donné, sinon Documents\CodeWalker* et Bureau.
pub fn find_codewalker_dll(hint: &str) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if !hint.is_empty() {
        let h = PathBuf::from(hint);
        cands.push(h.join("CodeWalker.Core.dll"));
        cands.push(h.clone());
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = PathBuf::from(home);
        for sub in ["Documents", "Desktop", "Downloads", "OneDrive\\Documents", ""] {
            let dir = home.join(sub);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    if name.contains("codewalker") {
                        cands.push(e.path().join("CodeWalker.Core.dll"));
                    }
                }
            }
        }
    }
    cands.into_iter().find(|p| p.is_file() && p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("CodeWalker.Core.dll")).unwrap_or(false))
}

/// Charge les clés : cache local, sinon GTA5.exe + CodeWalker.Core.dll.
pub fn load_keys(game_dir: &Path, codewalker_hint: &str, status: &dyn Fn(String)) -> Result<Keys, String> {
    let exe_path = ["GTA5.exe", "gta5.exe", "GTA5_Enhanced.exe"].iter().map(|n| game_dir.join(n)).find(|p| p.is_file()).ok_or("GTA5.exe introuvable dans ce dossier.")?;
    status("Lecture de GTA5.exe…".into());
    let exe = std::fs::read(&exe_path).map_err(|e| format!("Lecture de GTA5.exe impossible : {e}"))?;
    if let Some(k) = load_cache(&exe) {
        return Ok(k);
    }
    status("Recherche de la clé AES dans GTA5.exe…".into());
    let aes = find_aes_key(&exe).ok_or("Clé AES introuvable dans GTA5.exe (version du jeu non prise en charge ?).")?;
    let dll = find_codewalker_dll(codewalker_hint).ok_or("CodeWalker.Core.dll introuvable : indiquez le dossier de CodeWalker dans le panneau « Jeu ».")?;
    status("Lecture des tables de CodeWalker…".into());
    let dll_bytes = std::fs::read(&dll).map_err(|e| format!("Lecture de {} impossible : {e}", dll.display()))?;
    let magic = dotnet_resource_bytes(&dll_bytes, "magic").ok_or("Bloc « magic » introuvable dans CodeWalker.Core.dll.")?;
    let keys = decode_magic(&magic, &aes).ok_or("Décodage des tables impossible (CodeWalker trop ancien ?).")?;
    save_cache(&keys);
    Ok(keys)
}
