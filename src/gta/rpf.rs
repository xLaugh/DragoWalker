//! Lecture des archives RPF7 (RpfFile::ReadHeader 0x06005a9e, entrées 0x06005aec / 0x06005afd / 0x06005b0e,
//! extraction 0x06005aa4 / 0x06005aa5, taille des pages 0x06005b07). Lecture seule.

use super::crypto::{inflate, Keys};
use std::fs::File;
use std::os::windows::fs::FileExt;
use std::path::{Path, PathBuf};

const RPF7_MAGIC: u32 = 0x5250_4637; // 1380992567
const ENC_NONE: u32 = 0;
const ENC_OPEN: u32 = 0x4E45_504F; // 1313165391
const ENC_AES: u32 = 0x0FFF_FFF9; // 268435449
const DIR_MARKER: u32 = 0x7FFF_FF00; // 2147483392
/// bornes contre un en-tête abîmé ou piégé : nombre d'entrées, taille de la table des noms, archives imbriquées
const MAX_ENTRIES: usize = 1 << 22;
const MAX_NAMES: usize = 64 << 20;
const MAX_DEPTH: u32 = 8;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// size = taille compressée (0 = non compressé), usize = taille réelle.
    Binary { size: u32, usize: u32, encrypted: bool },
    Resource { size: u32, sys: u32, gfx: u32 },
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub archive: u32,
    /// nom en minuscules, avec extension
    pub name: String,
    /// dossier du fichier (archives comprises), partagé par tous les fichiers du dossier : le chemin complet n'est
    /// plus gardé pour chacun des ~390 000 fichiers ; path() le recompose
    pub dir: std::sync::Arc<str>,
    pub offset: u32,
    pub kind: Kind,
}

impl FileEntry {
    /// Chemin complet en minuscules (archives comprises).
    pub fn path(&self) -> String {
        format!("{}\\{}", self.dir, self.name)
    }
}

pub struct Archive {
    pub disk: usize,
    pub start: u64,
    pub aes: bool,
    #[allow(dead_code)]
    pub path: String,
}

pub struct Vfs {
    pub disks: Vec<File>,
    pub disk_paths: Vec<PathBuf>,
    pub archives: Vec<Archive>,
    pub files: Vec<FileEntry>,
    pub errors: Vec<String>,
}

fn u32le(d: &[u8], p: usize) -> u32 {
    d.get(p..p + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes).unwrap_or(0)
}

/// Taille d'un segment de ressource à partir de ses drapeaux (GetSizeFromFlags).
pub fn size_from_flags(f: u32) -> usize {
    let s0 = (f >> 27) & 1;
    let s1 = ((f >> 26) & 1) << 1;
    let s2 = ((f >> 25) & 1) << 2;
    let s3 = ((f >> 24) & 1) << 3;
    let s4 = ((f >> 17) & 0x7F) << 4;
    let s5 = ((f >> 11) & 0x3F) << 5;
    let s6 = ((f >> 7) & 0xF) << 6;
    let s7 = ((f >> 5) & 0x3) << 7;
    let s8 = ((f >> 4) & 0x1) << 8;
    let base = 0x200usize << (f & 0xF);
    base * (s0 + s1 + s2 + s3 + s4 + s5 + s6 + s7 + s8) as usize
}

impl Vfs {
    pub fn new() -> Vfs {
        Vfs { disks: Vec::new(), disk_paths: Vec::new(), archives: Vec::new(), files: Vec::new(), errors: Vec::new() }
    }

    fn read_at(&self, disk: usize, pos: u64, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut done = 0;
        while done < len {
            let n = self.disks[disk].seek_read(&mut buf[done..], pos + done as u64).ok()?;
            if n == 0 {
                return None;
            }
            done += n;
        }
        Some(buf)
    }

    /// Ouvre une archive posée sur le disque (et toutes celles qu'elle contient).
    pub fn open_disk_archive(&mut self, path: &Path, logical: &str, keys: &Keys) {
        let f = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                self.errors.push(format!("{} : {e}", path.display()));
                return;
            }
        };
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        self.disks.push(f);
        self.disk_paths.push(path.to_path_buf());
        let disk = self.disks.len() - 1;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.open_archive(disk, 0, &name, size, &logical.to_lowercase(), keys, 0);
    }

    /// `size` : taille de l'archive (0 = inconnue) ; `depth` : niveau d'imbrication (archive dans une archive).
    #[allow(clippy::too_many_arguments)]
    fn open_archive(&mut self, disk: usize, start: u64, name: &str, size: u64, logical: &str, keys: &Keys, depth: u32) {
        if depth > MAX_DEPTH {
            self.errors.push(format!("{logical} : trop d'archives imbriquées"));
            return;
        }
        let Some(h) = self.read_at(disk, start, 16) else {
            self.errors.push(format!("{logical} : en-tête illisible"));
            return;
        };
        if u32le(&h, 0) != RPF7_MAGIC {
            self.errors.push(format!("{logical} : pas une archive RPF7"));
            return;
        }
        let count = u32le(&h, 4) as usize;
        let names_len = u32le(&h, 8) as usize;
        let enc = u32le(&h, 12);
        // en-tête non vérifié : pas d'allocation démesurée, tables dans l'archive
        let tables = 16 + count as u64 * 16 + names_len as u64;
        if count > MAX_ENTRIES || names_len > MAX_NAMES || (size > 0 && tables > size) {
            self.errors.push(format!("{logical} : en-tête invalide ({count} entrées, {names_len} octets de noms)"));
            return;
        }
        let Some(mut entries) = self.read_at(disk, start + 16, count * 16) else { return };
        let Some(mut names) = self.read_at(disk, start + 16 + count as u64 * 16, names_len) else { return };
        let aes = enc == ENC_AES;
        match enc {
            ENC_NONE | ENC_OPEN => {}
            ENC_AES => {
                keys.decrypt_aes(&mut entries);
                keys.decrypt_aes(&mut names);
            }
            _ => {
                keys.decrypt_ng(&mut entries, name, size as u32);
                keys.decrypt_ng(&mut names, name, size as u32);
            }
        }
        let arch_idx = self.archives.len() as u32;
        self.archives.push(Archive { disk, start, aes, path: logical.to_string() });

        let read_name = |off: usize| -> String {
            if off >= names.len() {
                return String::new();
            }
            let end = names[off..].iter().position(|&b| b == 0).map(|e| off + e).unwrap_or(names.len());
            String::from_utf8_lossy(&names[off..end]).to_lowercase()
        };

        // Parcours des dossiers depuis la racine (entrée 0)
        if count == 0 || u32le(&entries, 4) != DIR_MARKER {
            self.errors.push(format!("{logical} : table des entrées illisible (clé ?)"));
            return;
        }
        let mut stack: Vec<(usize, std::sync::Arc<str>)> = vec![(0, std::sync::Arc::from(logical))];
        let mut nested: Vec<(u64, String, u64, String)> = Vec::new();
        // chaque entrée n'est lue qu'une fois : un dossier qui se contient lui-même (ou un parent) ne fait pas
        // tourner la lecture en rond
        let mut seen = vec![false; count];
        seen[0] = true;
        while let Some((di, dpath)) = stack.pop() {
            let e = &entries[di * 16..di * 16 + 16];
            let first = u32le(e, 8) as usize;
            let n = u32le(e, 12) as usize;
            for i in first.min(count)..first.saturating_add(n).min(count) {
                if std::mem::replace(&mut seen[i], true) {
                    continue;
                }
                let e = &entries[i * 16..i * 16 + 16];
                let h2 = u32le(e, 4);
                if h2 == DIR_MARKER {
                    let nm = read_name(u32le(e, 0) as usize);
                    stack.push((i, std::sync::Arc::from(format!("{dpath}\\{nm}"))));
                    continue;
                }
                let raw = u64::from_le_bytes(e[0..8].try_into().unwrap());
                if h2 & 0x8000_0000 == 0 {
                    let name_off = (raw & 0xFFFF) as usize;
                    let fsize = ((raw >> 16) & 0xFF_FFFF) as u32;
                    let offset = ((raw >> 40) & 0xFF_FFFF) as u32;
                    let usize_ = u32le(e, 8);
                    let encrypted = u32le(e, 12) == 1;
                    let nm = read_name(name_off);
                    let inner_size = if fsize > 0 { fsize } else { usize_ } as u64;
                    let pos = start + offset as u64 * 512;
                    // archive imbriquée : forcément après l'en-tête de celle-ci et dedans (sinon elle pourrait se
                    // rouvrir elle-même)
                    if nm.ends_with(".rpf") && pos > start && (size == 0 || pos - start + inner_size <= size) {
                        nested.push((pos, nm.clone(), inner_size, format!("{dpath}\\{nm}")));
                    }
                    self.files.push(FileEntry { archive: arch_idx, name: nm, dir: dpath.clone(), offset, kind: Kind::Binary { size: fsize, usize: usize_, encrypted } });
                } else {
                    let name_off = u16::from_le_bytes([e[0], e[1]]) as usize;
                    let mut fsize = e[2] as u32 | (e[3] as u32) << 8 | (e[4] as u32) << 16;
                    let offset = (e[5] as u32 | (e[6] as u32) << 8 | (e[7] as u32) << 16) & 0x7F_FFFF;
                    let sys = u32le(e, 8);
                    let gfx = u32le(e, 12);
                    if fsize == 0xFF_FFFF {
                        if let Some(b) = self.read_at(disk, start + offset as u64 * 512, 16) {
                            fsize = b[7] as u32 | (b[14] as u32) << 8 | (b[5] as u32) << 16 | (b[2] as u32) << 24;
                        }
                    }
                    let nm = read_name(name_off);
                    self.files.push(FileEntry { archive: arch_idx, name: nm, dir: dpath.clone(), offset, kind: Kind::Resource { size: fsize, sys, gfx } });
                }
            }
        }
        for (pos, nm, sz, path) in nested {
            self.open_archive(disk, pos, &nm, sz, &path, keys, depth + 1);
        }
    }

    /// Contenu d'un fichier binaire (déchiffré et décompressé).
    pub fn read_binary(&self, f: &FileEntry, keys: &Keys) -> Option<Vec<u8>> {
        let Kind::Binary { size, usize, encrypted } = f.kind else { return None };
        let a = &self.archives[f.archive as usize];
        let len = if size > 0 { size } else { usize } as usize;
        let mut data = self.read_at(a.disk, a.start + f.offset as u64 * 512, len)?;
        if encrypted {
            if a.aes {
                keys.decrypt_aes(&mut data);
            } else {
                keys.decrypt_ng(&mut data, &f.name, usize);
            }
        }
        if size > 0 {
            inflate(&data)
        } else {
            Some(data)
        }
    }

    /// Fichier ressource tel qu'il est dans l'archive (en-tête RSC7 + données compressées).
    /// Dans l'archive, les 16 premiers octets ne sont pas un en-tête lisible (CodeWalker les saute) :
    /// on reconstruit « RSC7 », la version (4 bits hauts des drapeaux) et les drapeaux de l'entrée.
    pub fn raw_resource_file(&self, f: &FileEntry) -> Option<Vec<u8>> {
        let Kind::Resource { size, sys, gfx } = f.kind else { return None };
        if size <= 16 {
            return None;
        }
        let a = &self.archives[f.archive as usize];
        let body = self.read_at(a.disk, a.start + f.offset as u64 * 512 + 16, size as usize - 16)?;
        let version = (((sys >> 28) & 0xF) << 4) | ((gfx >> 28) & 0xF);
        let mut d = Vec::with_capacity(body.len() + 16);
        d.extend_from_slice(b"RSC7");
        d.extend_from_slice(&version.to_le_bytes());
        d.extend_from_slice(&sys.to_le_bytes());
        d.extend_from_slice(&gfx.to_le_bytes());
        d.extend_from_slice(&body);
        Some(d)
    }

    /// Contenu d'une ressource (RSC7) : données décompressées + drapeaux système/graphiques.
    pub fn read_resource(&self, f: &FileEntry, keys: &Keys) -> Option<Resource> {
        let Kind::Resource { size, sys, gfx } = f.kind else { return None };
        let a = &self.archives[f.archive as usize];
        if size <= 16 {
            return None;
        }
        let mut data = self.read_at(a.disk, a.start + f.offset as u64 * 512 + 16, size as usize - 16)?;
        if f.name.ends_with(".ysc") {
            if a.aes {
                keys.decrypt_aes(&mut data);
            } else {
                keys.decrypt_ng(&mut data, &f.name, size);
            }
        }
        let data = inflate(&data)?;
        Some(Resource::new(data, sys, gfx))
    }
}

/// Ressource décompressée : segment système puis segment graphique.
pub struct Resource {
    pub data: Vec<u8>,
    pub sys_size: usize,
}

impl Resource {
    pub fn new(data: Vec<u8>, sys: u32, _gfx: u32) -> Resource {
        let sys_size = size_from_flags(sys);
        Resource { data, sys_size }
    }

    /// Convertit un pointeur de ressource (0x5… système, 0x6… graphique) en position dans `data`.
    pub fn ptr(&self, p: u64) -> Option<usize> {
        if p == 0 {
            return None;
        }
        let off = (p & 0x0FFF_FFFF) as usize;
        let pos = match p >> 28 {
            5 => off,
            6 => self.sys_size + off,
            _ => return None,
        };
        (pos < self.data.len()).then_some(pos)
    }

    pub fn u8(&self, p: usize) -> u8 {
        self.data.get(p).copied().unwrap_or(0)
    }
    pub fn u16(&self, p: usize) -> u16 {
        self.data.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0)
    }
    pub fn u32(&self, p: usize) -> u32 {
        self.data.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
    }
    pub fn i32(&self, p: usize) -> i32 {
        self.u32(p) as i32
    }
    pub fn u64(&self, p: usize) -> u64 {
        self.data.get(p..p + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
    }
    pub fn f32(&self, p: usize) -> f32 {
        f32::from_bits(self.u32(p))
    }
    pub fn v3(&self, p: usize) -> [f32; 3] {
        [self.f32(p), self.f32(p + 4), self.f32(p + 8)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys { aes: [0; 32], ng_keys: Vec::new(), ng_tables: Vec::new(), lut: [0; 256] }
    }

    fn dir(name_off: u32, first: u32, n: u32) -> Vec<u8> {
        [name_off, DIR_MARKER, first, n].iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn file(name_off: u16, size: u32, offset: u32) -> Vec<u8> {
        let raw = name_off as u64 | (size as u64 & 0xFF_FFFF) << 16 | (offset as u64 & 0xFF_FFFF) << 40;
        let mut v = raw.to_le_bytes().to_vec();
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v
    }

    fn archive(entries: &[Vec<u8>], names: &[u8], count_override: Option<u32>) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&RPF7_MAGIC.to_le_bytes());
        d.extend_from_slice(&count_override.unwrap_or(entries.len() as u32).to_le_bytes());
        d.extend_from_slice(&(names.len() as u32).to_le_bytes());
        d.extend_from_slice(&ENC_OPEN.to_le_bytes());
        for e in entries {
            d.extend_from_slice(e);
        }
        d.extend_from_slice(names);
        d.resize(4096, 0);
        d
    }

    fn open(data: &[u8], tag: &str) -> Vfs {
        let path = std::env::temp_dir().join(format!("dw_rpf_{tag}_{}.rpf", std::process::id()));
        std::fs::write(&path, data).unwrap();
        let mut v = Vfs::new();
        v.open_disk_archive(&path, "x.rpf", &keys());
        drop(v.disks.drain(..));
        let _ = std::fs::remove_file(&path);
        v
    }

    #[test]
    fn dossiers_en_boucle_et_archive_qui_se_contient() {
        // noms : "" (racine), "a", "f.txt", "moi.rpf"
        let names = b"\0a\0f.txt\0moi.rpf\0";
        let entries = vec![
            dir(0, 1, 3),       // racine : a, f.txt, moi.rpf
            dir(1, 0, 3),       // « a » se contient lui-même et contient la racine
            file(3, 10, 2),     // f.txt
            file(9, 4096, 0),   // moi.rpf : pointe sur l'archive elle-même
        ];
        let v = open(&archive(&entries, names, None), "boucle");
        let n: Vec<String> = v.files.iter().map(|f| f.path()).collect();
        assert_eq!(n.iter().filter(|p| p.ends_with("f.txt")).count(), 1, "{n:?}");
        assert_eq!(v.archives.len(), 1, "l'archive ne se rouvre pas elle-même");
    }

    #[test]
    fn en_tete_demesure_refuse() {
        let entries = vec![dir(0, 1, 0)];
        // 200 millions d'entrées annoncées dans un fichier de 4 Ko : refusé sans rien allouer
        let v = open(&archive(&entries, b"\0", Some(200_000_000)), "enorme");
        assert!(v.files.is_empty() && v.archives.is_empty());
        assert!(v.errors.iter().any(|e| e.contains("en-tête invalide")), "{:?}", v.errors);
        // plus d'entrées que la taille du fichier n'en permet
        let v = open(&archive(&entries, b"\0", Some(1000)), "trop");
        assert!(v.archives.is_empty());
    }
}
