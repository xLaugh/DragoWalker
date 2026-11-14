//! Export du projet : XML (format CodeWalker), puis fichiers binaires du jeu grâce au CodeWalker
//! installé (pont `cwbridge`). Les ymaps copiés du jeu récupèrent les sections du fichier d'origine
//! (générateurs de véhicules, lumières, occlusion, herbe…) et les effets de leurs objets.
//!
//! Chaque export écrit dans un dossier à lui (nom du projet nettoyé, `resource_name`) et y garde la liste des
//! fichiers qu'il a écrits (`EXPORT_LIST`) : à l'export suivant, ceux de la liste qui ne sont plus produits (carte
//! vidée ou supprimée, intérieur renommé, XML d'une conversion ratée…) sont retirés ; un fichier que DragoWalker n'a
//! pas écrit n'est jamais touché.

use crate::app::DwApp;
use crate::cwbridge;
use crate::model::Ymap;
use crate::xmlio;
use std::path::{Path, PathBuf};

/// Liste des fichiers écrits par le dernier export (chemins relatifs au dossier exporté, en minuscules, « / » ;
/// un dossier finit par « / »).
pub const EXPORT_LIST: &str = "dragowalker_fichiers.txt";
/// Début du nom des dossiers où un export met de côté les XML déjà convertis (dans le dossier exporté).
const PARKED_PREFIX: &str = ".dw_deja_converti";

/// Nom de dossier / de ressource FiveM tiré du nom du projet : minuscules, lettres sans accent, chiffres, « _ » et
/// « - » ; le reste devient « _ ». Vide : « dragowalker_map ».
pub fn resource_name(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        let c = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            'œ' => {
                s.push_str("oe");
                continue;
            }
            'æ' => {
                s.push_str("ae");
                continue;
            }
            c if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' => c,
            _ => '_',
        };
        // pas deux « _ » de suite
        if c == '_' && s.ends_with('_') {
            continue;
        }
        s.push(c);
    }
    let s = s.trim_matches(|c| c == '_' || c == '-').to_string();
    if s.is_empty() {
        "dragowalker_map".into()
    } else {
        s
    }
}

/// Liste de l'export précédent (None : pas de liste, dossier exporté par une version plus ancienne ou jamais).
pub fn read_export_list(out: &Path) -> Option<Vec<String>> {
    let s = std::fs::read_to_string(out.join(EXPORT_LIST)).ok()?;
    Some(s.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.to_lowercase()).collect())
}

fn write_export_list(out: &Path, list: &[String]) {
    let mut s = String::from("# DragoWalker : fichiers écrits par le dernier export (ceux qui ne sont plus produits sont retirés au suivant)\n");
    for l in list {
        s += l;
        s.push('\n');
    }
    let _ = std::fs::write(out.join(EXPORT_LIST), s);
}

/// Fichiers écrits par cet export (modifiés depuis `since`) dans `out`, et ceux qui en découlent : binaire de chaque
/// XML de `stream`, sa place dans sources_xml, son dossier de textures, réglages audio rangés dans « audio ». Les
/// dossiers de textures comptent en entier (« stream/nom/ »).
pub fn produced_files(out: &Path, stream: &Path, since: std::time::SystemTime) -> Vec<String> {
    let prefix = if stream == out { String::new() } else { format!("{}/", stream.strip_prefix(out).map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase()).unwrap_or_else(|_| "stream".into())) };
    let fresh = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).map(|t| t >= since).unwrap_or(false);
    let mut list: Vec<String> = Vec::new();
    // XML de stream écrits par cet export : leurs noms de base (dossiers de textures)
    let mut stems: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in std::fs::read_dir(stream).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        if n.ends_with(".xml") && e.path().is_file() && fresh(&e.path()) {
            stems.insert(n.split('.').next().unwrap_or("").to_string());
            let bin = n.trim_end_matches(".xml").to_string();
            list.push(format!("{prefix}{bin}"));
            list.push(format!("sources_xml/{n}"));
            if bin == crate::audio::AUDIO_REL {
                list.push(format!("audio/{bin}"));
            }
        }
    }
    fn walk(dir: &Path, rel: &str, depth: u32, ctx: &mut dyn FnMut(&Path, String, bool) -> bool) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let r = format!("{rel}{}", e.file_name().to_string_lossy().to_lowercase());
            if p.is_dir() {
                if ctx(&p, format!("{r}/"), true) && depth < 6 {
                    walk(&p, &format!("{r}/"), depth + 1, ctx);
                }
            } else {
                ctx(&p, r, false);
            }
        }
    }
    let stream_rel = prefix.clone();
    walk(out, "", 0, &mut |p, r, is_dir| {
        if is_dir {
            // pas de descente : sources_xml (déduit des XML), dossiers de mise de côté
            if r == "sources_xml/" || r.trim_end_matches('/').rsplit('/').next().unwrap_or("").starts_with(PARKED_PREFIX) {
                return false;
            }
            // dossier de textures d'un XML de cet export : compté en entier, ici et dans sources_xml
            if let Some(name) = r.strip_prefix(stream_rel.as_str()).map(|x| x.trim_end_matches('/')) {
                if !name.contains('/') && stems.contains(name) {
                    list.push(r.clone());
                    list.push(format!("sources_xml/{name}/"));
                    return false;
                }
            }
            return true;
        }
        if r == EXPORT_LIST || !fresh(p) {
            return false;
        }
        list.push(r);
        false
    });
    list.sort();
    list.dedup();
    list
}

/// Chemin relatif d'une liste d'export sans danger (dans le dossier exporté).
fn safe_rel(e: &str) -> bool {
    !(e.is_empty() || e.starts_with('/') || e.starts_with('\\') || e.contains(':') || e.split(['/', '\\']).any(|p| p == ".." || p == "."))
}

/// Retire de `out` ce que l'export précédent avait écrit (`old`) et que celui-ci ne produit plus (`new`). Rend le
/// nombre de fichiers ou dossiers retirés.
pub fn remove_stale(out: &Path, old: &[String], new: &[String]) -> usize {
    let keep: std::collections::HashSet<&str> = new.iter().map(|s| s.as_str()).collect();
    let mut n = 0;
    for e in old.iter().filter(|e| !keep.contains(e.as_str()) && safe_rel(e) && e.as_str() != EXPORT_LIST) {
        let p = out.join(e.trim_end_matches('/'));
        let gone = if e.ends_with('/') { p.is_dir() && std::fs::remove_dir_all(&p).is_ok() } else { p.is_file() && std::fs::remove_file(&p).is_ok() };
        n += gone as usize;
    }
    n
}

/// Convertit les XML du dossier stream en fichiers binaires du jeu (CodeWalker) puis range les XML et les dossiers
/// de textures dans sources_xml. Rend (tout converti sans erreur, compte rendu). Sans DwApp : peut tourner dans un
/// fil à part. Quand stream est le dossier exporté lui-même (export XML hors FiveM), seuls les XML et dossiers de
/// la liste de l'export sont rangés.
pub fn convert_and_tidy(dll: &Path, stream: &Path, out: &Path) -> (bool, Vec<String>) {
    let mut report = Vec::new();
    let listed: Option<std::collections::HashSet<String>> = if stream == out { Some(read_export_list(out).unwrap_or_default().into_iter().collect()) } else { None };
    // déjà converti au dernier export : CodeWalker fabrique chaque fichier seulement à partir de son XML et de son
    // dossier de textures (XmlMeta::GetData 0x06003faf ; GetYdrData 0x06003fb9 / GetYtdData 0x06003fb8 avec le
    // dossier) ; XML et dossier identiques à ceux rangés dans sources_xml, et binaire présent → binaire gardé
    let src = out.join("sources_xml");
    let same_dir = |a: &Path, b: &Path| -> bool {
        let list = |d: &Path| -> Vec<(std::ffi::OsString, Vec<u8>)> {
            let mut v: Vec<_> = std::fs::read_dir(d).into_iter().flatten().flatten().filter(|e| e.path().is_file()).map(|e| (e.file_name(), std::fs::read(e.path()).unwrap_or_default())).collect();
            v.sort_by(|x, y| x.0.cmp(&y.0));
            v
        };
        list(a) == list(b)
    };
    let mut reused = 0;
    // mis de côté dans le dossier exporté (même disque : un simple renommage) ; un dossier par conversion
    let parked = out.join(format!("{PARKED_PREFIX}_{}_{}", std::process::id(), cwbridge::unique()));
    let _ = std::fs::create_dir_all(&parked);
    for e in std::fs::read_dir(stream).into_iter().flatten().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let Some(bin) = name.strip_suffix(".xml") else { continue };
        let dbg = |why: &str| {
            if std::env::var("DW_DEBUG_CACHE").is_ok() {
                println!("   reconverti ({why}) : {name}");
            }
        };
        if !p.is_file() || !stream.join(bin).is_file() {
            dbg("pas de binaire");
            continue;
        }
        let Ok(prev) = std::fs::read(src.join(&name)) else {
            dbg("pas d'ancien XML");
            continue;
        };
        if std::fs::read(&p).map(|cur| cur != prev).unwrap_or(true) {
            if std::env::var("DW_DEBUG_CACHE").is_ok() {
                println!("   changé : {name}");
            }
            continue;
        }
        let stem = name.split('.').next().unwrap_or("").to_string();
        let tex = stream.join(&stem);
        if tex.is_dir() && !same_dir(&tex, &src.join(&stem)) {
            dbg("textures changées");
            continue;
        }
        // mis de côté le temps de la conversion des autres, puis rangé avec eux dans sources_xml ; compté seulement
        // s'il a vraiment été mis de côté (sinon il est simplement reconverti)
        if std::fs::rename(&p, parked.join(&name)).is_ok() {
            reused += 1;
        }
    }
    let result = cwbridge::to_binary(dll, stream);
    for e in std::fs::read_dir(&parked).into_iter().flatten().flatten() {
        let _ = std::fs::rename(e.path(), stream.join(e.file_name()));
    }
    // vide une fois tout remis en place (sinon gardé : rien n'est perdu)
    let _ = std::fs::remove_dir(&parked);
    if reused > 0 {
        report.push(format!("{reused} fichier(s) inchangé(s) depuis le dernier export, gardés sans reconversion."));
    }
    let res = match result {
        Ok((ok, errs)) => {
            let binary_ok = errs.is_empty() && (ok > 0 || reused > 0);
            report.push(format!("{ok} fichier(s) convertis en binaires du jeu par CodeWalker."));
            for e in errs.iter().take(5) {
                report.push(format!("Erreur : {e}"));
            }
            if binary_ok {
                // réglages audio des intérieurs : dans « audio », hors de stream (dossier des fichiers diffusés), comme
                // les ressources d'intérieur de la communauté ; dans stream, le jeu ne les chargeait pas (la pluie et la
                // rue s'entendaient dedans : retour de la personne, 13 nov. 2026)
                let rel = stream.join(crate::audio::AUDIO_REL);
                if rel.is_file() && stream != out {
                    let dir = out.join("audio");
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::remove_file(dir.join(crate::audio::AUDIO_REL));
                    if std::fs::rename(&rel, dir.join(crate::audio::AUDIO_REL)).is_err() {
                        report.push("Son des intérieurs : déplacement dans « audio » impossible.".into());
                    }
                }
                // les XML (et dossiers de textures) passent dans « sources_xml » ; dossier exporté = stream : seulement
                // ceux de cet export
                let _ = std::fs::create_dir_all(&src);
                if let Ok(rd) = std::fs::read_dir(stream) {
                    for e in rd.flatten() {
                        let p = e.path();
                        let low = e.file_name().to_string_lossy().to_lowercase();
                        if low == "sources_xml" || low.starts_with(PARKED_PREFIX) {
                            continue;
                        }
                        let is_xml = p.extension().map(|x| x.eq_ignore_ascii_case("xml")).unwrap_or(false) && p.is_file();
                        let ours = match &listed {
                            None => true,
                            Some(l) => l.contains(&if p.is_dir() { format!("{low}/") } else { low.clone() }),
                        };
                        if (is_xml || p.is_dir()) && ours {
                            let dest = src.join(e.file_name());
                            if dest.is_dir() {
                                let _ = std::fs::remove_dir_all(&dest);
                            }
                            let _ = std::fs::rename(&p, dest);
                        }
                    }
                }
            }
            (binary_ok, report)
        }
        Err(e) => {
            report.push(format!("Conversion en binaires impossible : {e}"));
            (false, report)
        }
    };
    let (ok, mut report) = res;
    // réglages audio déclarés dans le fxmanifest mais pas convertis (ou pas rangés) : déclaration retirée, sinon
    // FiveM cherche un fichier absent
    let man = out.join("fxmanifest.lua");
    let lines = crate::audio::audio_manifest_lines();
    if stream != out && !out.join("audio").join(crate::audio::AUDIO_REL).is_file() {
        if let Ok(m) = std::fs::read_to_string(&man) {
            if m.contains(&lines) && std::fs::write(&man, m.replace(&lines, "")).is_ok() {
                report.push(format!("Son des intérieurs : {} pas converti, retiré du fxmanifest (son XML reste dans stream).", crate::audio::AUDIO_REL));
            }
        }
    }
    (ok, report)
}

impl DwApp {
    /// Ymaps du projet prêts à exporter, avec ce que le fichier d'origine du jeu contenait en plus.
    /// Chaque intérieur posé dans une carte avec d'autres objets passe dans sa propre petite carte
    /// « <intérieur>_milo_ » (seule dans le manifeste à être INTERIOR_DATA), comme les 24 MLO FiveM analysés le
    /// 14 nov. 2026 (G:\MLOFIVEM : un placement par carte, sans autre objet) ; drapeaux du placement 1572864 comme
    /// eux. Pas pour une carte du jeu modifiée ni pour les cartes d'une ressource MLO importée.
    fn split_interior_maps(&self, ymaps: Vec<Ymap>, report: &mut Vec<String>) -> Vec<Ymap> {
        let res_maps: std::collections::HashSet<String> = self.proj.mlo_res.iter().flat_map(|r| r.ymaps.iter().map(|m| m.to_lowercase())).collect();
        let mut out = Vec::with_capacity(ymaps.len());
        let mut extra: Vec<Ymap> = Vec::new();
        let mut used: std::collections::HashSet<String> = ymaps.iter().map(|y| y.name.to_lowercase()).collect();
        for mut y in ymaps {
            let mixed = y.entities.iter().any(|e| e.mlo.is_some()) && y.entities.iter().any(|e| e.mlo.is_none());
            if y.from_game || res_maps.contains(&y.name.to_lowercase()) || !mixed || y.entities.iter().any(|e| e.parent_index >= 0) {
                out.push(y);
                continue;
            }
            let (inst, rest): (Vec<_>, Vec<_>) = y.entities.drain(..).partition(|e| e.mlo.is_some());
            y.entities = rest;
            for mut e in inst {
                if e.flags == 0 || e.flags == crate::mech::ENTITY_FLAGS {
                    e.flags = crate::mech::MLO_INSTANCE_FLAGS;
                }
                let base = format!("{}_milo_", e.archetype.to_lowercase());
                let mut name = base.clone();
                let mut k = 2;
                while used.contains(&name) {
                    name = format!("{base}{k}");
                    k += 1;
                }
                used.insert(name.clone());
                let mut m = Ymap::new(&name);
                m.entities.push(e);
                extra.push(m);
            }
            out.push(y);
        }
        if !extra.is_empty() {
            report.push(format!("{} intérieur(s) posé(s) chacun dans sa propre carte « …_milo_ » (comme les MLO FiveM).", extra.len()));
        }
        out.extend(extra);
        out
    }

    fn ymaps_for_export(&self, dll: Option<&Path>, report: &mut Vec<String>) -> Vec<Ymap> {
        // une carte qui n'a que des boîtes d'occlusion est exportée aussi
        let mut out: Vec<Ymap> = self.proj.ymaps.iter().filter(|y| !y.sample && (!y.entities.is_empty() || !y.car_gens.is_empty() || !y.occluders.is_empty() || y.from_game)).cloned().collect();
        // test en jeu : parties laissées de côté (cartes d'un MLO, végétation posée au pinceau)
        let skipped_maps: Vec<String> = self.proj.mlo_res.iter().filter(|r| self.export_skip.contains(&format!("mlo:{}", r.name))).flat_map(|r| r.ymaps.iter().cloned()).collect();
        out.retain(|y| !skipped_maps.contains(&y.name));
        if self.export_skip.iter().any(|s| s == "veg") {
            let veg: std::collections::HashSet<&str> = crate::vegetation::SETS.iter().flat_map(|s| s.props.iter().copied()).collect();
            for y in out.iter_mut() {
                y.entities.retain(|e| !veg.contains(e.archetype.to_lowercase().as_str()));
            }
        }
        let (Some(dll), Some(w)) = (dll, self.game.world.as_ref()) else {
            if out.iter().any(|y| y.from_game) {
                report.push("CodeWalker introuvable : les sections d'origine (véhicules, lumières…) des cartes du jeu modifiées ne sont pas reprises.".into());
            }
            return out;
        };
        let tmp = std::env::temp_dir().join("dragowalker_cw").join("origines");
        let _ = std::fs::create_dir_all(&tmp);
        let mut pairs: Vec<(PathBuf, PathBuf)> = Vec::new();
        for y in out.iter().filter(|y| y.from_game) {
            let Some(wy) = w.ymaps.iter().filter(|g| g.name == y.name.to_lowercase()).max_by_key(|g| (g.in_project, g.prio)) else { continue };
            let f = &w.vfs.files[wy.file];
            if std::env::var("DW_TEST_EXPORT").is_ok() {
                println!("origine {} : {} ({:?})", y.name, f.path(), w.vfs.raw_resource_file(f).map(|r| (r.len(), r.get(..8).map(|b| b.to_vec()).unwrap_or_default())));
            }
            if let Some(raw) = w.vfs.raw_resource_file(f) {
                let bin = tmp.join(format!("{}.ymap", y.name));
                // même contenu qu'au dernier export : fichier laissé tel quel, pour que sa lecture en XML reste en cache
                let same = std::fs::read(&bin).map(|old| old == raw).unwrap_or(false);
                if same || std::fs::write(&bin, raw).is_ok() {
                    pairs.push((bin, tmp.join(format!("{}.orig.ymap.xml", y.name))));
                }
            }
        }
        if pairs.is_empty() {
            return out;
        }
        match cwbridge::to_xml(dll, &pairs) {
            Ok(_) => {
                let mut merged = 0;
                for y in out.iter_mut().filter(|y| y.from_game) {
                    let Ok(src) = std::fs::read_to_string(tmp.join(format!("{}.orig.ymap.xml", y.name))) else { continue };
                    let Ok(orig) = xmlio::read_ymap(&src, &y.name) else { continue };
                    if y.raw_sections.is_empty() {
                        y.raw_sections = orig.raw_sections.clone();
                    }
                    if y.flags == 0 {
                        y.flags = orig.flags;
                    }
                    for e in y.entities.iter_mut() {
                        if e.game_index >= 0 && e.extensions_xml.is_empty() {
                            if let Some(o) = orig.entities.get(e.game_index as usize) {
                                e.extensions_xml = o.extensions_xml.clone();
                            }
                        }
                    }
                    merged += 1;
                }
                report.push(format!("{merged} carte(s) du jeu modifiée(s) : véhicules, lumières, occlusion et effets d'origine conservés."));
            }
            Err(e) => report.push(format!("Lecture des cartes d'origine impossible : {e}")),
        }
        out
    }

    /// Export manuel (menu Fichier) : comme `export_project`, mais la conversion par CodeWalker (≈ 30 s pour une
    /// grande map) se fait dans un fil à part, par le même chemin que le test en jeu (server.rs) ; l'avancement est
    /// affiché dans la barre d'état et le compte rendu à la fin. Rend le message à afficher tout de suite.
    /// (À appeler par files.rs::export_xml à la place de export_project ; `allow` à retirer une fois branché.)
    pub fn export_project_bg(&mut self, dir: &Path, fivem: bool) -> String {
        if self.server.job.is_some() {
            return "Une conversion est déjà en cours, attends qu'elle finisse.".into();
        }
        self.export_defer_convert = true;
        self.pending_convert = None;
        let rep = self.export_project(dir, fivem);
        self.export_defer_convert = false;
        if let Some((_, _, out)) = self.pending_convert.as_ref() {
            let out = out.clone();
            self.server_convert_export(out, fivem, rep.clone());
        }
        rep
    }

    /// Exporte le projet dans `dir` (dans un dossier à son nom, `resource_name`). Rend le compte rendu affiché à la
    /// personne.
    pub fn export_project(&mut self, dir: &Path, fivem: bool) -> String {
        // marge d'une seconde : précision des dates de fichiers
        let export_start = std::time::SystemTime::now() - std::time::Duration::from_secs(1);
        // intérieurs créés rangés par erreur dans le .ytyp d'une ressource importée : remis dans un .ytyp du projet
        let moved = self.fix_misplaced_mlos();
        // intérieurs créés : chaque pièce a son objet « coque » (sol, murs, plafond) avant d'écrire les .ytyp
        self.ensure_shell_entities();
        // toujours un dossier à part, même pour l'export XML : rien n'est rangé ni retiré dans le dossier choisi
        let out = dir.join(resource_name(&self.proj.name));
        let stream = if fivem { out.join("stream") } else { out.clone() };
        if let Err(e) = std::fs::create_dir_all(&stream) {
            return format!("Impossible de créer le dossier : {e}");
        }
        let old_list = read_export_list(&out);
        let dll = cwbridge::dll(&self.game.cw_dir);
        let mut report = Vec::new();
        if moved > 0 {
            report.push(format!("{moved} intérieur(s) créé(s) dans DragoWalker étaient rangés dans le fichier de types d'une ressource MLO importée (recopié tel quel, donc jamais exportés) : remis dans un fichier du projet. Enregistre le projet."));
        }
        let ymaps = self.ymaps_for_export(dll.as_deref(), &mut report);
        let ymaps = self.split_interior_maps(ymaps, &mut report);
        let mut n = 0;
        // cartes du projet : les bâtiments du jeu copiés emportent leur version lointaine (carte « _lod »)
        let mut lod_ymaps: Vec<crate::model::Ymap> = Vec::new();
        let mut lod_n = 0;
        let mut written_maps: std::collections::HashSet<String> = std::collections::HashSet::new();
        for y in &ymaps {
            let (hd, lod) = if y.parent.is_empty() {
                let (h, l, k) = self.lod_split(y);
                lod_n += k;
                (h, l)
            } else {
                (y.clone(), None)
            };
            if std::fs::write(stream.join(format!("{}.ymap.xml", hd.name)), xmlio::write_ymap(&hd, &self.proj)).is_ok() {
                n += 1;
                written_maps.insert(hd.name.to_lowercase());
            }
            if let Some(l) = lod {
                if std::fs::write(stream.join(format!("{}.ymap.xml", l.name)), xmlio::write_ymap(&l, &self.proj)).is_ok() {
                    n += 1;
                    written_maps.insert(l.name.to_lowercase());
                }
                lod_ymaps.push(l);
            }
        }
        // cartes du projet plus exportées (vidées, laissées de côté) : leurs fichiers d'un export précédent retirés,
        // sinon le jeu chargerait encore l'ancienne version
        let mut emptied = 0;
        for y in self.proj.ymaps.iter().filter(|y| !y.sample && !written_maps.contains(&y.name.to_lowercase())) {
            for f in [format!("{}.ymap", y.name), format!("{}.ymap.xml", y.name)] {
                if std::fs::remove_file(stream.join(&f)).is_ok() {
                    emptied += 1;
                }
            }
        }
        if emptied > 0 {
            report.push(format!("{emptied} fichier(s) de carte vide ou laissée de côté retiré(s) du dossier exporté."));
        }
        if lod_n > 0 {
            report.push(format!("{lod_n} bâtiment(s) copié(s) avec leur version lointaine du jeu (visibles de loin comme dans GTA V)."));
        }
        // les intérieurs des MLO importés dont le .ytyp du créateur est là sont recopiés tels quels (export_mlos) ;
        // fourni seulement en XML ou introuvable : réécrit depuis le projet
        let copied = self.mlo_copied_ytyps();
        let external = self.mlo_external_ytyps();
        // test en jeu : .ytyp des MLO laissés de côté
        let skipped_ytyps: Vec<String> = self.proj.mlo_res.iter().filter(|r| self.export_skip.contains(&format!("mlo:{}", r.name))).flat_map(|r| r.ytyp_files.iter().map(|f| f.0.to_lowercase())).collect();
        let skip = |k: &str| self.export_skip.iter().any(|s| s == k);
        let mut rewritten: Vec<String> = Vec::new();
        for t in self.proj.ytyps.iter().filter(|t| !copied.contains(&t.name.to_lowercase()) && !skipped_ytyps.contains(&t.name.to_lowercase())) {
            if std::fs::write(stream.join(format!("{}.ytyp.xml", t.name)), xmlio::write_ytyp(t)).is_ok() {
                n += 1;
                if external.contains(&t.name.to_lowercase()) {
                    rewritten.push(t.name.clone());
                }
            }
        }
        if !rewritten.is_empty() {
            report.push(format!("MLO importé : {} réécrit(s) depuis le projet (fichier binaire du créateur introuvable ou fourni seulement en XML).", rewritten.join(", ")));
        }
        n += self.copy_custom_models(&stream);
        // terrain de l'éditeur : modèle, collisions, type d'objet, placement (eau et effets compris)
        for old in ["dw_water.xml", "dw_water.lua"] {
            let _ = std::fs::remove_file(out.join(old));
        }
        let mut terrain_ytyp: Option<String> = None;
        match self.export_terrain(&stream) {
            Ok(v) if !v.is_empty() => {
                n += 4;
                report.push(v[0].clone());
                terrain_ytyp = v.get(1).cloned();
            }
            Ok(_) => {}
            Err(e) => report.push(format!("Export du terrain impossible : {e}")),
        }
        if skip("water") {
            for old in ["dw_water.xml", "dw_water.lua"] {
                let _ = std::fs::remove_file(out.join(old));
            }
        }
        // circulation : points de route générés depuis les routes du terrain (nodesNNN.ynd) ; anciens carrés : retirés
        // par la liste de l'export (sans liste, dossier stream d'une ressource : tous, comme avant)
        if old_list.is_none() && stream != out {
            if let Ok(rd) = std::fs::read_dir(&stream) {
                for f in rd.flatten() {
                    let n = f.file_name().to_string_lossy().to_lowercase();
                    if n.starts_with("nodes") && n.contains(".ynd") && f.path().is_file() {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
        }
        if let Some(t) = self.proj.terrain.as_ref() {
            // carrés que le jeu a déjà : jamais remplacés (voir traffic::export_ynds) ; sans le jeu chargé, on ne peut
            // pas savoir : aucun carré n'est écrit
            let w = self.game.world.as_deref();
            let game_has = |a: u32| w.map(|w| w.vfs.files.iter().any(|f| f.name.eq_ignore_ascii_case(&format!("nodes{a}.ynd")))).unwrap_or(true);
            match crate::traffic::export_ynds(t, &stream, &game_has) {
                Ok((0, 0)) => {}
                Ok((k, s)) => {
                    if k > 0 {
                        report.push(format!("Circulation : points de route écrits pour {k} carré(s) de 512 m où le jeu n'en a pas."));
                    }
                    if s > 0 {
                        report.push(format!(
                            "Circulation : {s} carré(s) gardent les points de route du jeu (les remplacer faisait planter le jeu, ses carrés voisins y étant reliés){}.",
                            if w.is_none() { " — charge le jeu avant d'exporter pour savoir où le jeu n'en a pas" } else { "" }
                        ));
                    }
                }
                Err(e) => report.push(format!("Export de la circulation impossible : {e}")),
            }
        }
        // intérieurs créés dans DragoWalker : collision (sol de chaque pièce, numéro de pièce) — avant le manifeste,
        // qui ne liste que les collisions vraiment écrites
        report.extend(self.export_interior_bounds(&stream));
        let fresh = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).map(|t| t >= export_start).unwrap_or(false);
        let interior_bounds: Vec<String> = self
            .proj
            .ytyps
            .iter()
            .filter(|t| !external.contains(&t.name.to_lowercase()))
            .flat_map(|t| t.mlos.iter())
            .map(|m| m.name.clone())
            .filter(|n| fresh(&stream.join(format!("{}.ybn.xml", n.to_lowercase()))))
            .collect();
        // bâtiments du jeu copiés : dépendances .ytyp et collisions
        match self.export_buildings(&stream, &ymaps, &lod_ymaps, &interior_bounds) {
            Ok(Some(m)) => report.push(m),
            Ok(None) => {}
            Err(e) => report.push(format!("Export des bâtiments copiés impossible : {e}")),
        }
        // MLO importés : fichiers du créateur, collisions déplacées, crédits
        let (mlo_report, mut mlo_manifest) = self.export_mlos(&out, &stream, dll.as_deref());
        report.extend(mlo_report);
        // contrôle des intérieurs et compteur de performance (check.rs)
        report.extend(self.check_report());
        // bâtiments du jeu édités (morceaux retirés à la main) : modèles découpés, types, collisions
        let (cut_report, cut_manifest) = self.export_cuts(&stream, dll.as_deref());
        report.extend(cut_report);
        mlo_manifest += &cut_manifest;
        // coques des intérieurs créés (shell.rs)
        let (shell_report, shell_manifest) = self.export_shells(&stream);
        report.extend(shell_report);
        mlo_manifest += &shell_manifest;
        // bâtiments de l'atelier (atelier.rs)
        let (house_report, house_manifest) = self.export_houses(&stream);
        report.extend(house_report);
        mlo_manifest += &house_manifest;
        // collision d'intérieur listée dans _manifest.ymf mais retirée depuis (l'atelier efface les XML « dw_bat_* »
        // de l'export précédent, y compris celle-ci) : plus déclarée, le jeu chercherait un fichier absent
        let gone: Vec<&String> = interior_bounds.iter().filter(|n| !stream.join(format!("{}.ybn.xml", n.to_lowercase())).is_file()).collect();
        if !gone.is_empty() {
            let ymf = stream.join("_manifest.ymf.xml");
            if let Ok(mut s) = std::fs::read_to_string(&ymf) {
                for n in &gone {
                    s = s.replace(&format!("    <Item>\n      <Name>{n}</Name>\n      <Bounds>\n        <Item>{n}</Item>\n      </Bounds>\n    </Item>\n"), "");
                }
                let _ = std::fs::write(&ymf, s);
            }
            report.push(format!("Intérieur(s) sans collision écrite, pas déclarés dans _manifest.ymf : {}.", gone.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
        // son des intérieurs créés (occlusion et réglages audio des pièces, audio.rs)
        let (audio_report, audio_manifest) = self.export_audio(&stream, &out, dll.is_some());
        report.extend(audio_report);
        mlo_manifest += &audio_manifest;
        // dossier exporté par une version sans liste : binaires fabriqués par DragoWalker qui ne correspondent plus à
        // rien (tuile, route, collision ou façade disparue), reconnus à leur nom ; jamais un fichier recopié d'un MLO
        // importé (ses fichiers d'occlusion « <nombre>.ymt » gardent la date d'origine)
        if old_list.is_none() {
            let res_prefixes: Vec<String> = self.proj.mlo_res.iter().map(|r| r.name.replace(' ', "_").to_lowercase()).flat_map(|r| [format!("{r}_x_"), format!("{r}_exterieur")]).collect();
            let mlo_files: std::collections::HashSet<String> = self.proj.mlo_res.iter().flat_map(|r| r.assets.iter().chain(r.ybns.iter())).filter_map(|a| Path::new(a).file_name().map(|n| n.to_string_lossy().to_lowercase())).collect();
            for f in std::fs::read_dir(&stream).into_iter().flatten().flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                let ours = ["dw_terrain", "dw_road_", "dw_bcol_", "dw_fx_", "dw_cut", "dw_shell", "dw_bat_", "dw_house", "dw_audio_game", "_manifest.ymf"].iter().any(|p| n.starts_with(p)) || n.strip_suffix(".ymt").map(|b| b.chars().all(|c| c.is_ascii_digit())).unwrap_or(false) || res_prefixes.iter().any(|p| n.starts_with(p.as_str()));
                // écrit par cet export (ex. façade copiée en entier, sans XML) : gardé
                if ours && !mlo_files.contains(&n) && !fresh(&f.path()) && !n.ends_with(".xml") && f.path().is_file() && !stream.join(format!("{}.xml", f.file_name().to_string_lossy())).exists() {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
        if fivem {
            let mut man = String::from("fx_version 'cerulean'\ngame 'gta5'\n\nthis_is_a_map 'yes'\n\nfiles {\n    'stream/*.ytyp'\n}\n\n");
            for t in self.proj.ytyps.iter().filter(|t| !skipped_ytyps.contains(&t.name.to_lowercase())) {
                man += &format!("data_file 'DLC_ITYP_REQUEST' 'stream/{}.ytyp'\n", t.name);
            }
            if let Some(t) = &terrain_ytyp {
                man += &format!("data_file 'DLC_ITYP_REQUEST' 'stream/{t}.ytyp'\n");
            }
            if !mlo_manifest.is_empty() {
                man += &format!("\n{mlo_manifest}");
            }
            for t in std::fs::read_dir(&self.sollumz.dir).into_iter().flatten().flatten() {
                let n = t.file_name().to_string_lossy().to_lowercase();
                if let Some(stem) = n.strip_suffix(".ytyp.xml") {
                    man += &format!("data_file 'DLC_ITYP_REQUEST' 'stream/{stem}.ytyp'\n");
                }
            }
            // voitures de PNJ qui roulent : script client
            let _ = std::fs::remove_file(out.join("dw_traffic.lua"));
            if let Some(t) = self.proj.terrain.as_ref().filter(|_| !skip("traffic")) {
                match crate::traffic::export_npc_script(t, &out) {
                    Ok(true) => man += "\nclient_script 'dw_traffic.lua'\n",
                    Ok(false) => {}
                    Err(e) => report.push(format!("Export des voitures de PNJ impossible : {e}")),
                }
            }
            // PNJ : script client
            if skip("peds") {
                let _ = std::fs::remove_file(out.join("dw_peds.lua"));
            } else {
                match self.export_peds(&out) {
                    Ok(true) => man += "\nclient_script 'dw_peds.lua'\n",
                    Ok(false) => {}
                    Err(e) => report.push(format!("Export des PNJ impossible : {e}")),
                }
            }
            // portes des intérieurs : enregistrées dans le système de portes du jeu, ouvertes
            match self.export_doors(&out) {
                Ok(n) if n > 0 => {
                    man += "
client_script 'dw_doors.lua'
";
                    report.push(format!("Portes : {n} porte(s) d'intérieur enregistrée(s) et ouverte(s) en jeu (dw_doors.lua) ; configurations ox_doorlock / qb-doorlock dans doorlock/ (à copier, voir LISEZMOI.txt)."));
                }
                Ok(_) => {}
                Err(e) => report.push(format!("Script des portes impossible : {e}")),
            }
            // variantes des intérieurs (entity sets) : exemple de script client, actives par défaut celles cochées
            match self.export_entity_sets(&out) {
                Ok(n) if n > 0 => {
                    man += "\nclient_script 'dw_entity_sets.lua'\n";
                    report.push(format!("Variantes : {n} intérieur(s) avec des variantes (dw_entity_sets.lua, exemple pour les activer)."));
                }
                Ok(_) => {}
                Err(e) => report.push(format!("Script des variantes impossible : {e}")),
            }
            if !self.export_skip.is_empty() {
                report.push(format!("Test en jeu sans : {}.", self.export_skip.join(", ")));
            }
            // ambiances du projet utilisées par les pièces : timecycle modifiers (ambiance.rs)
            match crate::ambiance::export_file(&self.proj, &out) {
                Ok(0) => {}
                Ok(n) => {
                    let f = crate::ambiance::TC_FILE;
                    man += &format!("\nfiles {{ '{f}' }}\ndata_file 'TIMECYCLEMOD_FILE' '{f}'\n");
                    report.push(format!("Ambiances du projet : {n} écrite(s) dans {f}."));
                }
                Err(e) => report.push(format!("Ambiances du projet impossibles à écrire : {e}")),
            }
            // eau du terrain : water.xml complet chargé par un script client (native FiveM LoadWaterFromPath)
            if out.join("dw_water.xml").exists() {
                man += "\nfiles { 'dw_water.xml' }\nclient_script 'dw_water.lua'\n";
            }
            let _ = std::fs::write(out.join("fxmanifest.lua"), man);
        }
        // liste de cet export ; ce que le précédent avait écrit et qui n'est plus produit est retiré (avant la
        // conversion : un XML resté d'une conversion ratée ne doit pas être reconverti)
        let mut list = produced_files(&out, &stream, export_start);
        if fivem {
            list.push("lisezmoi.txt".into());
            list.sort();
            list.dedup();
        }
        if let Some(old) = &old_list {
            let k = remove_stale(&out, old, &list);
            if k > 0 {
                report.push(format!("{k} fichier(s) d'un export précédent qui ne sont plus produits retiré(s)."));
            }
        }
        write_export_list(&out, &list);
        // conversion en fichiers binaires du jeu par CodeWalker (la partie longue : ≈ 30 s pour une grande map) ;
        // test en jeu et export manuel : faite dans un fil à part (server.rs), DragoWalker reste utilisable
        let mut binary_ok = false;
        match &dll {
            Some(d) if self.export_defer_convert => {
                self.pending_convert = Some((d.clone(), stream.clone(), out.clone()));
                binary_ok = true;
                report.push("Conversion en binaires du jeu en cours (en arrière-plan)…".into());
            }
            Some(d) => {
                let (ok, rep) = convert_and_tidy(d, &stream, &out);
                binary_ok = ok;
                report.extend(rep);
            }
            None => report.push("CodeWalker introuvable : fichiers laissés en XML (indique son dossier dans l'onglet Jeu).".into()),
        }
        if fivem {
            let readme = if binary_ok {
                "Ressource FiveM préparée par DragoWalker.\n\n\
                 Le dossier stream contient les fichiers binaires du jeu (.ymap, .ytyp, .ydr), convertis par CodeWalker.\n\
                 Les sources XML sont dans sources_xml (pour CodeWalker ou Sollumz).\n\
                 Copiez ce dossier dans resources/ et ajoutez « ensure <nom> » à server.cfg.\n"
            } else {
                "Ressource FiveM préparée par DragoWalker.\n\n\
                 Les fichiers du dossier stream sont en XML (format CodeWalker).\n\
                 FiveM a besoin des versions binaires : ouvrez-les avec CodeWalker (Importer XML),\n\
                 ou indiquez le dossier de CodeWalker dans l'onglet Jeu et exportez à nouveau.\n"
            };
            let mut readme = readme.to_string();
            if let Some(t) = self.proj.terrain.as_ref() {
                // conseils repris du TEST_FIVEM.md du vaisseau (E:\Blender-Code\vaisseau_mlo), validés en jeu
                let (mn, mx) = t.bounds();
                let c = (mn + mx) * 0.5;
                readme.push_str(&format!(
                    "\nTerrain : textures dans dw_terrain_txd.ytd, une collision par tuile (dw_terrain_col_*.ybn).\n\
                     Après chaque nouvel export, videz le cache de FiveM (AppData/Local/FiveM/FiveM.app/data/cache).\n\
                     Pour y aller : /tp {:.0} {:.0} {:.0} — si vous tombez au travers, attendez 1 à 2 secondes\n\
                     ou appelez RequestCollisionAtCoord({:.1}, {:.1}, {:.1}) avant de vous téléporter.\n",
                    c.x,
                    c.y,
                    mx.z + 3.0,
                    c.x,
                    c.y,
                    mx.z + 3.0
                ));
            }
            let _ = std::fs::write(out.join("LISEZMOI.txt"), readme);
        }
        format!("{n} fichier(s) exporté(s) dans {}. {}", out.display(), report.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_de_ressource() {
        assert_eq!(resource_name("Nouveau projet"), "nouveau_projet");
        assert_eq!(resource_name("  Ma Map d'Été !  "), "ma_map_d_ete");
        assert_eq!(resource_name("[maps] Cœur/..\\x"), "maps_coeur_x");
        assert_eq!(resource_name("garage-2"), "garage-2");
        assert_eq!(resource_name(""), "dragowalker_map");
        assert_eq!(resource_name("¤¤ ##"), "dragowalker_map");
    }

    #[test]
    fn nettoyage_par_liste() {
        let base = std::env::temp_dir().join(format!("dw_test_liste_{}_{}", std::process::id(), cwbridge::unique()));
        let out = base.join("res");
        let stream = out.join("stream");
        std::fs::create_dir_all(stream.join("tex")).unwrap();
        std::fs::create_dir_all(out.join("sources_xml")).unwrap();
        // fichiers d'un export précédent, et un fichier que DragoWalker n'a jamais écrit
        for f in ["stream/vieille.ymap", "stream/garde.ymap", "stream/tex/a.dds", "sources_xml/vieille.ymap.xml", "stream/perso.ydr", "stream/123456.ymt"] {
            std::fs::write(out.join(f), b"x").unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        let since = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        // cet export : garde.ymap.xml (son binaire reste), tex.ydr.xml (et son dossier de textures), le manifeste
        std::fs::write(stream.join("garde.ymap.xml"), b"<x/>").unwrap();
        std::fs::write(stream.join("tex.ydr.xml"), b"<x/>").unwrap();
        std::fs::write(out.join("fxmanifest.lua"), b"--").unwrap();
        let new = produced_files(&out, &stream, since);
        for e in ["stream/garde.ymap", "stream/garde.ymap.xml", "sources_xml/garde.ymap.xml", "stream/tex.ydr", "stream/tex/", "sources_xml/tex/", "fxmanifest.lua"] {
            assert!(new.contains(&e.to_string()), "{e} absent de {new:?}");
        }
        assert!(!new.iter().any(|e| e.contains("perso") || e.contains("123456") || e.contains("vieille")), "{new:?}");
        let old: Vec<String> = ["stream/vieille.ymap", "sources_xml/vieille.ymap.xml", "stream/garde.ymap", "../dehors.txt", "c:/windows/x"].iter().map(|s| s.to_string()).collect();
        std::fs::write(base.join("dehors.txt"), b"x").unwrap();
        assert_eq!(remove_stale(&out, &old, &new), 2);
        assert!(!stream.join("vieille.ymap").exists());
        assert!(!out.join("sources_xml/vieille.ymap.xml").exists());
        // gardés : produit maintenant, jamais écrit par DragoWalker, hors du dossier
        assert!(stream.join("garde.ymap").exists());
        assert!(stream.join("perso.ydr").exists());
        assert!(stream.join("123456.ymt").exists());
        assert!(base.join("dehors.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
