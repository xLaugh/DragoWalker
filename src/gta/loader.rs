//! Chargement des modèles 3D en arrière-plan (plusieurs fils), avec leurs textures.
//! Ordre de recherche d'une texture : dictionnaire intégré au modèle, puis dictionnaire du type
//! d'objet (champ textureDictionary de CBaseArchetypeDef, fichier .ytd du même hachage).

use super::crypto::Keys;
use super::drawable::{self, RModel, TexData};
use super::rpf::{Resource, Vfs};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Ce qu'il faut pour lire les modèles depuis plusieurs fils.
#[derive(Clone)]
pub struct ModelSource {
    pub vfs: Arc<Vfs>,
    pub keys: Arc<Keys>,
    /// ydr / ydd / yft par hachage de nom
    pub models: Arc<HashMap<u32, Vec<usize>>>,
    /// ytd par hachage de nom
    pub txds: Arc<HashMap<u32, Vec<usize>>>,
    /// type d'objet → (dictionnaire de textures, dictionnaire de modèles)
    pub arch: Arc<HashMap<u32, (u32, u32)>>,
    /// dictionnaire de textures → parent (gtxd)
    pub txd_parents: Arc<HashMap<u32, u32>>,
}

pub struct Loaded {
    pub arch: u32,
    pub model: Option<RModel>,
    /// clé de texture de chaque partie (0 = aucune)
    pub part_keys: Vec<u64>,
    pub textures: Vec<(u64, TexData)>,
}

/// Taille maximale gardée pour une texture (pixels).
pub const MAX_TEX: u32 = 256;
/// Nombre maximal de triangles lus par modèle.
pub const MAX_TRIS: usize = 120000;

fn key(dict: u32, name: u32) -> u64 {
    ((dict as u64) << 32) | name as u64
}

impl ModelSource {
    fn resource(&self, fi: usize) -> Option<Resource> {
        self.vfs.read_resource(&self.vfs.files[fi], &self.keys)
    }

    /// Fichier et position du modèle d'un type d'objet.
    fn find_model(&self, arch: u32) -> Option<(usize, Resource, usize)> {
        if let Some(list) = self.models.get(&arch) {
            for &fi in list.iter().rev() {
                let f = &self.vfs.files[fi];
                let ext = f.name.rsplit('.').next().unwrap_or("");
                let Some(r) = self.resource(fi) else { continue };
                if let Some(p) = drawable::drawable_pos(&r, ext, arch) {
                    return Some((fi, r, p));
                }
            }
        }
        let dict = self.arch.get(&arch).map(|a| a.1).unwrap_or(0);
        if dict != 0 {
            if let Some(list) = self.models.get(&dict) {
                for &fi in list.iter().rev() {
                    if !self.vfs.files[fi].name.ends_with(".ydd") {
                        continue;
                    }
                    let Some(r) = self.resource(fi) else { continue };
                    if let Some(p) = drawable::drawable_pos(&r, "ydd", arch) {
                        return Some((fi, r, p));
                    }
                }
            }
        }
        None
    }

    /// Lumières rangées dans le modèle d'un type d'objet (vide pour les yft).
    pub fn lights(&self, arch: u32) -> Vec<drawable::GameLight> {
        match self.find_model(arch) {
            Some((fi, r, pos)) if !self.vfs.files[fi].name.ends_with(".yft") => drawable::read_lights(&r, pos),
            _ => Vec::new(),
        }
    }

    pub fn load(&self, arch: u32, claimed: &Mutex<HashSet<u64>>, ytd_cache: &mut Vec<(u32, Arc<Resource>)>) -> Loaded {
        let mut out = Loaded { arch, model: None, part_keys: Vec::new(), textures: Vec::new() };
        let Some((fi, r, pos)) = self.find_model(arch) else { return out };
        let Some(model) = drawable::read_render_drawable(&r, pos, MAX_TRIS) else { return out };
        let txd = self.arch.get(&arch).map(|a| a.0).unwrap_or(0);
        let mut found: HashMap<u32, u64> = HashMap::new();
        for g in &model.geoms {
            if g.tex == 0 || found.contains_key(&g.tex) {
                continue;
            }
            let mut k = 0u64;
            // 1. textures intégrées au modèle
            if let Some(d) = model.embedded_dict {
                let kk = key(0x8000_0000 | fi as u32, g.tex);
                if claimed.lock().map(|c| c.contains(&kk)).unwrap_or(false) {
                    k = kk;
                } else if let Some(t) = drawable::find_texture(&r, d, g.tex, MAX_TEX) {
                    if claimed.lock().map(|mut c| c.insert(kk)).unwrap_or(false) {
                        out.textures.push((kk, t));
                    }
                    k = kk;
                }
            }
            // 2. dictionnaire du type d'objet, puis ses parents (gtxd)
            let mut cur = txd;
            let mut depth = 0;
            while k == 0 && cur != 0 && depth < 8 {
                let kk = key(cur, g.tex);
                if claimed.lock().map(|c| c.contains(&kk)).unwrap_or(false) {
                    k = kk;
                    break;
                }
                if let Some(res) = self.ytd(cur, ytd_cache) {
                    if let Some(t) = drawable::find_texture(&res, 0, g.tex, MAX_TEX) {
                        if claimed.lock().map(|mut c| c.insert(kk)).unwrap_or(false) {
                            out.textures.push((kk, t));
                        }
                        k = kk;
                        break;
                    }
                }
                cur = self.txd_parents.get(&cur).copied().unwrap_or(0);
                depth += 1;
            }
            found.insert(g.tex, k);
        }
        out.part_keys = model.geoms.iter().map(|g| found.get(&g.tex).copied().unwrap_or(0)).collect();
        out.model = Some(model);
        out
    }

    fn ytd(&self, h: u32, cache: &mut Vec<(u32, Arc<Resource>)>) -> Option<Arc<Resource>> {
        if let Some(i) = cache.iter().position(|c| c.0 == h) {
            let e = cache.remove(i);
            let r = e.1.clone();
            cache.push(e);
            return Some(r);
        }
        let fi = *self.txds.get(&h)?.last()?;
        let r = Arc::new(self.resource(fi)?);
        cache.push((h, r.clone()));
        if cache.len() > 24 {
            cache.remove(0);
        }
        Some(r)
    }
}

/// Lance `n` fils de chargement : on leur envoie des types d'objets, ils rendent des modèles.
pub fn spawn_workers(src: ModelSource, n: usize, claimed: Arc<Mutex<HashSet<u64>>>) -> (Sender<u32>, Receiver<Loaded>) {
    let (req_tx, req_rx) = std::sync::mpsc::channel::<u32>();
    let (res_tx, res_rx) = std::sync::mpsc::channel::<Loaded>();
    let req_rx = Arc::new(Mutex::new(req_rx));
    for _ in 0..n {
        let src = src.clone();
        let rx = req_rx.clone();
        let tx = res_tx.clone();
        let claimed = claimed.clone();
        std::thread::spawn(move || {
            let mut cache = Vec::new();
            loop {
                let arch = match rx.lock() {
                    Ok(g) => match g.recv() {
                        Ok(a) => a,
                        Err(_) => return,
                    },
                    Err(_) => return,
                };
                let l = src.load(arch, &claimed, &mut cache);
                if tx.send(l).is_err() {
                    return;
                }
            }
        });
    }
    (req_tx, res_rx)
}
