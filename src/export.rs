//! Export du projet : XML (format CodeWalker), puis fichiers binaires du jeu grâce au CodeWalker
//! installé (pont `cwbridge`). Les ymaps copiés du jeu récupèrent les sections du fichier d'origine
//! (générateurs de véhicules, lumières, occlusion, herbe…) et les effets de leurs objets.

use crate::app::DwApp;
use crate::cwbridge;
use crate::model::Ymap;
use crate::xmlio;
use std::path::{Path, PathBuf};

impl DwApp {
    /// Ymaps du projet prêts à exporter, avec ce que le fichier d'origine du jeu contenait en plus.
    fn ymaps_for_export(&self, dll: Option<&Path>, report: &mut Vec<String>) -> Vec<Ymap> {
        let mut out: Vec<Ymap> = self.proj.ymaps.iter().filter(|y| !y.sample && (!y.entities.is_empty() || y.from_game)).cloned().collect();
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
                println!("origine {} : {} ({:?})", y.name, f.path, w.vfs.raw_resource_file(f).map(|r| (r.len(), r[..8].to_vec())));
            }
            if let Some(raw) = w.vfs.raw_resource_file(f) {
                let bin = tmp.join(format!("{}.ymap", y.name));
                if std::fs::write(&bin, raw).is_ok() {
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

    /// Exporte le projet dans `dir`. Rend le compte rendu affiché à la personne.
    pub fn export_project(&mut self, dir: &Path, fivem: bool) -> String {
        let out = if fivem { dir.join(self.proj.name.replace(' ', "_").to_lowercase()) } else { dir.to_path_buf() };
        let stream = if fivem { out.join("stream") } else { out.clone() };
        if let Err(e) = std::fs::create_dir_all(&stream) {
            return format!("Impossible de créer le dossier : {e}");
        }
        let dll = cwbridge::dll(&self.game.cw_dir);
        let mut report = Vec::new();
        let ymaps = self.ymaps_for_export(dll.as_deref(), &mut report);
        let mut n = 0;
        for y in &ymaps {
            if std::fs::write(stream.join(format!("{}.ymap.xml", y.name)), xmlio::write_ymap(y, &self.proj)).is_ok() {
                n += 1;
            }
        }
        for t in &self.proj.ytyps {
            if std::fs::write(stream.join(format!("{}.ytyp.xml", t.name)), xmlio::write_ytyp(t)).is_ok() {
                n += 1;
            }
        }
        n += self.copy_custom_models(&stream);
        // conversion en fichiers binaires du jeu par CodeWalker
        let mut binary_ok = false;
        match &dll {
            Some(d) => match cwbridge::to_binary(d, &stream) {
                Ok((ok, errs)) => {
                    binary_ok = errs.is_empty() && ok > 0;
                    report.push(format!("{ok} fichier(s) convertis en binaires du jeu par CodeWalker."));
                    for e in errs.iter().take(5) {
                        report.push(format!("Erreur : {e}"));
                    }
                    if binary_ok {
                        // les XML (et dossiers de textures) passent dans « sources_xml »
                        let src = out.join("sources_xml");
                        let _ = std::fs::create_dir_all(&src);
                        if let Ok(rd) = std::fs::read_dir(&stream) {
                            for e in rd.flatten() {
                                let p = e.path();
                                let is_xml = p.extension().map(|x| x.eq_ignore_ascii_case("xml")).unwrap_or(false);
                                if is_xml || p.is_dir() {
                                    let _ = std::fs::rename(&p, src.join(e.file_name()));
                                }
                            }
                        }
                    }
                }
                Err(e) => report.push(format!("Conversion en binaires impossible : {e}")),
            },
            None => report.push("CodeWalker introuvable : fichiers laissés en XML (indique son dossier dans l'onglet Jeu).".into()),
        }
        if fivem {
            let mut man = String::from("fx_version 'cerulean'\ngame 'gta5'\n\nthis_is_a_map 'yes'\n\nfiles {\n    'stream/*.ytyp'\n}\n\n");
            for t in &self.proj.ytyps {
                man += &format!("data_file 'DLC_ITYP_REQUEST' 'stream/{}.ytyp'\n", t.name);
            }
            for t in std::fs::read_dir(&self.sollumz.dir).into_iter().flatten().flatten() {
                let n = t.file_name().to_string_lossy().to_lowercase();
                if let Some(stem) = n.strip_suffix(".ytyp.xml") {
                    man += &format!("data_file 'DLC_ITYP_REQUEST' 'stream/{stem}.ytyp'\n");
                }
            }
            let _ = std::fs::write(out.join("fxmanifest.lua"), man);
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
            let _ = std::fs::write(out.join("LISEZMOI.txt"), readme);
        }
        format!("{n} fichier(s) exporté(s) dans {}. {}", out.display(), report.join(" "))
    }
}
