//! Export du projet : XML (format CodeWalker), puis fichiers binaires du jeu grâce au CodeWalker
//! installé (pont `cwbridge`). Les ymaps copiés du jeu récupèrent les sections du fichier d'origine
//! (générateurs de véhicules, lumières, occlusion, herbe…) et les effets de leurs objets.

use crate::app::DwApp;
use crate::cwbridge;
use crate::model::Ymap;
use crate::xmlio;
use std::path::{Path, PathBuf};

/// Convertit les XML du dossier stream en fichiers binaires du jeu (CodeWalker) puis range les XML et les dossiers
/// de textures dans sources_xml. Rend (tout converti sans erreur, compte rendu). Sans DwApp : peut tourner dans un
/// fil à part.
pub fn convert_and_tidy(dll: &Path, stream: &Path, out: &Path) -> (bool, Vec<String>) {
    let mut report = Vec::new();
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
    let parked = std::env::temp_dir().join("dragowalker_cw").join("deja_converti");
    let _ = std::fs::remove_dir_all(&parked);
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
        // mis de côté le temps de la conversion des autres, puis rangé avec eux dans sources_xml
        let _ = std::fs::rename(&p, parked.join(&name));
        reused += 1;
    }
    let result = cwbridge::to_binary(dll, stream);
    for e in std::fs::read_dir(&parked).into_iter().flatten().flatten() {
        let _ = std::fs::rename(e.path(), stream.join(e.file_name()));
    }
    if reused > 0 {
        report.push(format!("{reused} fichier(s) inchangé(s) depuis le dernier export, gardés sans reconversion."));
    }
    match result {
        Ok((ok, errs)) => {
            let binary_ok = errs.is_empty() && (ok > 0 || reused > 0);
            report.push(format!("{ok} fichier(s) convertis en binaires du jeu par CodeWalker."));
            for e in errs.iter().take(5) {
                report.push(format!("Erreur : {e}"));
            }
            if binary_ok {
                // les XML (et dossiers de textures) passent dans « sources_xml »
                let src = out.join("sources_xml");
                let _ = std::fs::create_dir_all(&src);
                if let Ok(rd) = std::fs::read_dir(stream) {
                    for e in rd.flatten() {
                        let p = e.path();
                        let is_xml = p.extension().map(|x| x.eq_ignore_ascii_case("xml")).unwrap_or(false);
                        if is_xml || p.is_dir() {
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
    }
}

impl DwApp {
    /// Ymaps du projet prêts à exporter, avec ce que le fichier d'origine du jeu contenait en plus.
    fn ymaps_for_export(&self, dll: Option<&Path>, report: &mut Vec<String>) -> Vec<Ymap> {
        let mut out: Vec<Ymap> = self.proj.ymaps.iter().filter(|y| !y.sample && (!y.entities.is_empty() || !y.car_gens.is_empty() || y.from_game)).cloned().collect();
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
                println!("origine {} : {} ({:?})", y.name, f.path(), w.vfs.raw_resource_file(f).map(|r| (r.len(), r[..8].to_vec())));
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

    /// Exporte le projet dans `dir`. Rend le compte rendu affiché à la personne.
    pub fn export_project(&mut self, dir: &Path, fivem: bool) -> String {
        // marge d'une seconde : précision des dates de fichiers
        let export_start = std::time::SystemTime::now() - std::time::Duration::from_secs(1);
        // intérieurs créés rangés par erreur dans le .ytyp d'une ressource importée : remis dans un .ytyp du projet
        let moved = self.fix_misplaced_mlos();
        // intérieurs créés : chaque pièce a son objet « coque » (sol, murs, plafond) avant d'écrire les .ytyp
        self.ensure_shell_entities();
        let out = if fivem { dir.join(self.proj.name.replace(' ', "_").to_lowercase()) } else { dir.to_path_buf() };
        let stream = if fivem { out.join("stream") } else { out.clone() };
        if let Err(e) = std::fs::create_dir_all(&stream) {
            return format!("Impossible de créer le dossier : {e}");
        }
        let dll = cwbridge::dll(&self.game.cw_dir);
        let mut report = Vec::new();
        if moved > 0 {
            report.push(format!("{moved} intérieur(s) créé(s) dans DragoWalker étaient rangés dans le fichier de types d'une ressource MLO importée (recopié tel quel, donc jamais exportés) : remis dans un fichier du projet. Enregistre le projet."));
        }
        let ymaps = self.ymaps_for_export(dll.as_deref(), &mut report);
        let mut n = 0;
        // cartes du projet : les bâtiments du jeu copiés emportent leur version lointaine (carte « _lod »)
        let mut lod_ymaps: Vec<crate::model::Ymap> = Vec::new();
        let mut lod_n = 0;
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
            }
            if let Some(l) = lod {
                if std::fs::write(stream.join(format!("{}.ymap.xml", l.name)), xmlio::write_ymap(&l, &self.proj)).is_ok() {
                    n += 1;
                }
                lod_ymaps.push(l);
            }
        }
        if lod_n > 0 {
            report.push(format!("{lod_n} bâtiment(s) copié(s) avec leur version lointaine du jeu (visibles de loin comme dans GTA V)."));
        }
        // les intérieurs des MLO importés sont recopiés tels quels (export_mlos), pas réécrits
        let external = self.mlo_external_ytyps();
        // test en jeu : .ytyp des MLO laissés de côté
        let skipped_ytyps: Vec<String> = self.proj.mlo_res.iter().filter(|r| self.export_skip.contains(&format!("mlo:{}", r.name))).flat_map(|r| r.ytyp_files.iter().map(|f| f.0.to_lowercase())).collect();
        let skip = |k: &str| self.export_skip.iter().any(|s| s == k);
        for t in self.proj.ytyps.iter().filter(|t| !external.contains(&t.name.to_lowercase()) && !skipped_ytyps.contains(&t.name.to_lowercase())) {
            if std::fs::write(stream.join(format!("{}.ytyp.xml", t.name)), xmlio::write_ytyp(t)).is_ok() {
                n += 1;
            }
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
        // circulation : points de route générés depuis les routes du terrain (nodesNNN.ynd)
        if let Ok(rd) = std::fs::read_dir(&stream) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("nodes") && n.contains(".ynd") && f.path().is_file() {
                    let _ = std::fs::remove_file(f.path());
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
        // bâtiments du jeu copiés : dépendances .ytyp et collisions
        match self.export_buildings(&stream, &lod_ymaps) {
            Ok(Some(m)) => report.push(m),
            Ok(None) => {}
            Err(e) => report.push(format!("Export des bâtiments copiés impossible : {e}")),
        }
        // MLO importés : fichiers du créateur, collisions déplacées, crédits
        let (mlo_report, mut mlo_manifest) = self.export_mlos(&out, &stream, dll.as_deref());
        report.extend(mlo_report);
        // bâtiments du jeu édités (morceaux retirés à la main) : modèles découpés, types, collisions
        // intérieurs créés dans DragoWalker : collision (sol de chaque pièce, numéro de pièce)
        report.extend(self.export_interior_bounds(&stream));
        // contrôle des intérieurs et compteur de performance (check.rs)
        report.extend(self.check_report());
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
        // son des intérieurs créés (occlusion et réglages audio des pièces, audio.rs)
        let (audio_report, audio_manifest) = self.export_audio(&stream);
        report.extend(audio_report);
        mlo_manifest += &audio_manifest;
        // binaires fabriqués par DragoWalker lors d'un export précédent et qui ne correspondent plus à rien (tuile,
        // route, collision ou façade disparue) : retirés ; les autres restent, pour être réutilisés s'ils n'ont pas
        // changé (convert_and_tidy)
        let res_prefixes: Vec<String> = self.proj.mlo_res.iter().map(|r| r.name.replace(' ', "_").to_lowercase()).flat_map(|r| [format!("{r}_x_"), format!("{r}_exterieur")]).collect();
        for f in std::fs::read_dir(&stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            let ours = ["dw_terrain", "dw_road_", "dw_bcol_", "dw_fx_", "dw_cut", "dw_shell", "dw_bat_", "dw_house", "dw_audio_game", "_manifest.ymf"].iter().any(|p| n.starts_with(p)) || n.strip_suffix(".ymt").map(|b| b.chars().all(|c| c.is_ascii_digit())).unwrap_or(false) || res_prefixes.iter().any(|p| n.starts_with(p.as_str()));
            // écrit par cet export (ex. façade copiée en entier, sans XML) : gardé
            let fresh = f.metadata().and_then(|m| m.modified()).map(|t| t >= export_start).unwrap_or(true);
            if ours && !fresh && !n.ends_with(".xml") && f.path().is_file() && !stream.join(format!("{}.xml", f.file_name().to_string_lossy())).exists() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        // conversion en fichiers binaires du jeu par CodeWalker (la partie longue : ≈ 30 s pour une grande map) ;
        // test en jeu : faite dans un fil à part (server.rs), DragoWalker reste utilisable
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
                    report.push(format!("Portes : {n} porte(s) d'intérieur enregistrée(s) et ouverte(s) en jeu (dw_doors.lua)."));
                }
                Ok(_) => {}
                Err(e) => report.push(format!("Script des portes impossible : {e}")),
            }
            if !self.export_skip.is_empty() {
                report.push(format!("Test en jeu sans : {}.", self.export_skip.join(", ")));
            }
            // eau du terrain : water.xml complet chargé par un script client (native FiveM LoadWaterFromPath)
            if out.join("dw_water.xml").exists() {
                man += "\nfiles { 'dw_water.xml' }\nclient_script 'dw_water.lua'\n";
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
