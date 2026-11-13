//! Fichiers du projet : import, ouverture, enregistrement, export.
//! (Déplacé de app.rs sans modification.)

use crate::app::*;
use crate::math::{v3, V3};
use crate::model::{self, Project};
use crate::sample;
use crate::xmlio;

impl DwApp {
    // ------------------------------------------------------------ fichiers

    pub fn import_file(&mut self, path: &std::path::Path) {
        let Ok(src) = std::fs::read_to_string(path) else {
            self.status = format!("Impossible de lire {}", path.display());
            return;
        };
        let fname = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let base = fname.split('.').next().unwrap_or("import").to_string();
        if fname.to_lowercase().ends_with(".dwproj") {
            self.open_project_str(&src, path);
            return;
        }
        if src.contains("<CMapTypes") {
            match xmlio::read_ytyp(&src, &base) {
                Ok(t) => {
                    self.push_undo();
                    let n = t.mlos.len();
                    self.proj.ytyps.push(t);
                    model::bump_ids(self.proj.max_id());
                    self.status = format!("ytyp « {base} » importé ({n} intérieur(s)).");
                }
                Err(e) => self.status = e,
            }
        } else {
            match xmlio::read_ymap(&src, &base) {
                Ok(y) => {
                    self.push_undo();
                    let n = y.entities.len();
                    // centrer la vue sur le ymap importé et ajuster le sol
                    if n > 0 {
                        let mut zs: Vec<f32> = y.entities.iter().map(|e| e.pos.z).collect();
                        zs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let c = y.entities.iter().fold(V3::ZERO, |a, e| a + e.pos) * (1.0 / n as f32);
                        self.ground_z = zs[zs.len() / 4];
                        self.cam.target = v3(c.x, c.y, self.ground_z);
                        if c.dist2d(V3::ZERO) > sample::CITY_HALF * 2.0 {
                            self.opts.show_sample = false;
                        }
                    }
                    self.proj.ymaps.push(y);
                    self.target_ymap = self.proj.ymaps.len() - 1;
                    model::bump_ids(self.proj.max_id());
                    self.status = format!("ymap « {base} » importé ({n} objets). Il devient le ymap de destination.");
                }
                Err(e) => self.status = e,
            }
        }
    }

    pub fn open_project_str(&mut self, src: &str, path: &std::path::Path) {
        match serde_json::from_str::<Project>(src) {
            Ok(mut p) => {
                if !p.ymaps.iter().any(|y| y.sample) {
                    p.ymaps.insert(0, sample::generate());
                }
                self.proj = p;
                model::bump_ids(self.proj.max_id());
                self.undo.clear();
                self.redo.clear();
                self.set_selection(vec![]);
                self.target_ymap = self.proj.ymaps.iter().position(|y| !y.sample).unwrap_or(0);
                self.project_path = Some(path.to_path_buf());
                self.safe.mark_loaded();
                self.status = format!("Projet ouvert : {}", path.display());
                // projet d'avant la correction des rotations d'intérieur (CMloInstanceDef non inversé)
                let fixed = self.proj.fix_mlo_rotations();
                let moved = self.fix_misplaced_mlos();
                if moved > 0 {
                    self.status += &format!(" · {moved} intérieur(s) créé(s) rangé(s) dans le fichier d'un MLO importé (jamais exporté) : remis dans un fichier du projet, enregistre le projet.");
                }
                if fixed > 0 {
                    self.status += &format!(" · Orientation de {fixed} placement(s) d'intérieur corrigée (le tour donné avec les poignées est gardé) : enregistre le projet.");
                    self.sync_game_project();
                }
            }
            Err(e) => self.status = format!("Projet illisible : {e}"),
        }
    }

    pub fn save_project(&mut self, ask: bool) {
        let path = if ask || self.project_path.is_none() {
            rfd::FileDialog::new().add_filter("Projet DragoWalker", &["dwproj"]).set_file_name(format!("{}.dwproj", self.proj.name)).save_file()
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else { return };
        // la carte d'exemple est regénérée à l'ouverture
        let mut p = self.proj.clone();
        p.ymaps.retain(|y| !y.sample);
        match serde_json::to_string_pretty(&p) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(_) => {
                    self.status = format!("Projet enregistré : {}", path.display());
                    self.project_path = Some(path);
                    self.safe.mark_saved();
                }
                Err(e) => self.status = format!("Échec de l'enregistrement : {e}"),
            },
            Err(e) => self.status = format!("Échec : {e}"),
        }
    }

    pub fn export_xml(&mut self, fivem: bool) {
        let Some(dir) = rfd::FileDialog::new().set_title("Dossier d'export").pick_folder() else { return };
        self.status = self.export_project(&dir, fivem);
    }
}
