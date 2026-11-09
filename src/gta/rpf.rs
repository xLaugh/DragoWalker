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
    /// chemin complet en minuscules (archives comprises)
    pub path: String,
    pub offset: u32,
    pub kind: Kind,
}

pub struct Archive {
    pub disk: usize,
    pub start: u64,
    pub aes: bool,
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
    u32::from_le_bytes(d[p..p + 4].try_into().unwrap())
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
        self.open_archive(disk, 0, &name, size as u32, &logical.to_lowercase(), keys);
    }

    fn open_archive(&mut self, disk: usize, start: u64, name: &str, size: u32, logical: &str, keys: &Keys) {
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
                keys.decrypt_ng(&mut entries, name, size);
                keys.decrypt_ng(&mut names, name, size);
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
        let mut stack: Vec<(usize, String)> = vec![(0, logical.to_string())];
        let mut nested: Vec<(u64, String, u32, String)> = Vec::new();
        while let Some((di, dpath)) = stack.pop() {
            let e = &entries[di * 16..di * 16 + 16];
            let first = u32le(e, 8) as usize;
            let n = u32le(e, 12) as usize;
            for i in first..(first + n).min(count) {
                let e = &entries[i * 16..i * 16 + 16];
                let h2 = u32le(e, 4);
                if h2 == DIR_MARKER {
                    let nm = read_name(u32le(e, 0) as usize);
                    stack.push((i, format!("{dpath}\\{nm}")));
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
                    let path = format!("{dpath}\\{nm}");
                    if nm.ends_with(".rpf") {
                        let inner_size = if fsize > 0 { fsize } else { usize_ };
                        nested.push((start + offset as u64 * 512, nm.clone(), inner_size, path.clone()));
                    }
                    self.files.push(FileEntry { archive: arch_idx, name: nm, path, offset, kind: Kind::Binary { size: fsize, usize: usize_, encrypted } });
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
                    let path = format!("{dpath}\\{nm}");
                    self.files.push(FileEntry { archive: arch_idx, name: nm, path, offset, kind: Kind::Resource { size: fsize, sys, gfx } });
                }
            }
        }
        for (pos, nm, sz, path) in nested {
            self.open_archive(disk, pos, &nm, sz, &path, keys);
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
