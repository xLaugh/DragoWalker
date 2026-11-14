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
        if path.extension().map(|e| e.eq_ignore_ascii_case("dwproj")).unwrap_or(false) {
            // projet modifié : « Enregistrer les modifications ? » d'abord (safety.rs, pending_open_ui)
            if self.safe.dirty {
                self.safe.pending_open = Some(crate::safety::PendingOpen::File(path.to_path_buf()));
                return;
            }
            self.open_project_file(path);
            return;
        }
        let Ok(src) = std::fs::read_to_string(path) else {
            self.status = format!("Impossible de lire {}", path.display());
            return;
        };
        let fname = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let base = fname.split('.').next().unwrap_or("import").to_string();
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

    /// Ouvre un fichier .dwproj (sans question : voir import_file).
    pub fn open_project_file(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(src) => {
                self.open_project_str(&src, path);
            }
            Err(e) => self.status = format!("Impossible de lire {} : {e}", path.display()),
        }
    }

    /// Remplace le projet par celui du texte ; faux s'il est illisible (le projet actuel est alors gardé).
    pub fn open_project_str(&mut self, src: &str, path: &std::path::Path) -> bool {
        match parse_project(src) {
            Ok((mut p, nulls)) => {
                if !p.ymaps.iter().any(|y| y.sample) {
                    p.ymaps.insert(0, sample::generate());
                }
                sanitize_project(&mut p);
                self.proj = p;
                model::bump_ids(self.proj.max_id());
                self.undo.clear();
                self.redo.clear();
                self.set_selection(vec![]);
                self.target_ymap = self.proj.ymaps.iter().position(|y| !y.sample).unwrap_or(0);
                self.project_path = Some(path.to_path_buf());
                self.safe.mark_loaded();
                // le terrain du projet précédent ne doit pas rester affiché ; ce qui désignait ses routes est oublié
                self.terrain.dirty = true;
                self.terrain.road_pts.clear();
                self.terrain.road_edit = None;
                self.terrain.edit_drag = None;
                self.terrain.flat_target = None;
                self.terrain.veg_last = None;
                self.terrain.water_start = None;
                self.terrain.clip_a = None;
                self.status = format!("Projet ouvert : {}", path.display());
                if nulls > 0 {
                    self.status += &format!(" · {nulls} valeur(s) invalide(s) (NaN) remplacée(s) par 0 : vérifie les objets concernés.");
                }
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
                true
            }
            Err(e) => {
                self.status = format!("Projet illisible : {e}");
                false
            }
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
        sanitize_project(&mut p);
        match serde_json::to_string_pretty(&p) {
            Ok(s) => match write_project_file(&path, s.as_bytes()) {
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
        self.status = self.export_project_bg(&dir, fivem);
    }
}

/// Enregistre un projet : l'ancien fichier est gardé en « .dwproj.bak », puis le nouveau est écrit de façon
/// atomique (jamais de fichier à moitié écrit, même si l'app s'arrête pendant l'écriture).
pub fn write_project_file(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    if path.is_file() {
        let mut bak = path.as_os_str().to_os_string();
        bak.push(".bak");
        // copie (pas un renommage) : le fichier reste en place si l'écriture échoue
        std::fs::copy(path, std::path::PathBuf::from(bak)).map_err(|e| format!("copie de secours de {} : {e}", path.display()))?;
    }
    crate::safety::write_atomic(path, data)
}

/// Lit un projet. Un nombre invalide (NaN, infini) est écrit « null » par serde_json, qui refuse ensuite de le
/// relire : chaque « null » refusé là où un nombre est attendu est remplacé par 0. Renvoie le projet et le nombre
/// de valeurs remplacées.
pub fn parse_project(src: &str) -> Result<(Project, usize), String> {
    let mut text = std::borrow::Cow::Borrowed(src);
    let mut fixed = 0usize;
    loop {
        match serde_json::from_str::<Project>(&text) {
            Ok(p) => return Ok((p, fixed)),
            Err(e) => {
                let msg = e.to_string();
                let num = msg.contains("invalid type: null, expected f32") || msg.contains("invalid type: null, expected f64");
                if !num || fixed >= 100_000 {
                    return Err(msg);
                }
                let Some(at) = null_before(&text, e.line(), e.column()) else { return Err(msg) };
                text.to_mut().replace_range(at..at + 4, "0");
                fixed += 1;
            }
        }
    }
}

/// Position (octets) du « null » qui finit à la ligne et à la colonne données par serde_json.
fn null_before(text: &str, line: usize, column: usize) -> Option<usize> {
    let start = if line <= 1 { 0 } else { text.match_indices('\n').nth(line - 2)?.0 + 1 };
    let end = (start + column + 1).min(text.len());
    let at = start + text.get(start..end)?.rfind("null")?;
    Some(at)
}

/// Remplace les nombres invalides (NaN, infini) par des valeurs sûres avant d'écrire ou après avoir lu : position
/// 0, rotation neutre, échelle 1 ; hauteurs du terrain 0.
pub fn sanitize_project(p: &mut Project) {
    fn fix(v: &mut f32, d: f32) {
        if !v.is_finite() {
            *v = d;
        }
    }
    fn ent(e: &mut model::Entity) {
        fix(&mut e.pos.x, 0.0);
        fix(&mut e.pos.y, 0.0);
        fix(&mut e.pos.z, 0.0);
        let q = e.rot;
        if !(q.x.is_finite() && q.y.is_finite() && q.z.is_finite() && q.w.is_finite()) || q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w < 1e-12 {
            e.rot = crate::math::Quat::IDENTITY;
        }
        fix(&mut e.scale_xy, 1.0);
        fix(&mut e.scale_z, 1.0);
        fix(&mut e.lod_dist, 0.0);
        fix(&mut e.child_lod_dist, 0.0);
    }
    for y in &mut p.ymaps {
        y.entities.iter_mut().for_each(ent);
    }
    for t in &mut p.ytyps {
        for m in &mut t.mlos {
            m.entities.iter_mut().for_each(ent);
        }
    }
    if let Some(t) = &mut p.terrain {
        if t.h.iter().any(|v| !v.is_finite()) {
            std::sync::Arc::make_mut(&mut t.h).iter_mut().for_each(|v| fix(v, 0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, Ymap};

    fn projet() -> Project {
        let mut y = Ymap::new("map1");
        y.entities.push(Entity::new("prop_bench_01a", v3(1.0, 2.0, 3.0)));
        y.entities.push(Entity::new("prop_bench_01a", v3(4.0, 5.0, 6.0)));
        Project { name: "essai".into(), ymaps: vec![y], ..Default::default() }
    }

    #[test]
    fn nan_ecrit_null_puis_relu() {
        let mut p = projet();
        p.ymaps[0].entities[0].pos.x = f32::NAN;
        p.ymaps[0].entities[1].scale_xy = f32::INFINITY;
        p.ymaps[0].entities[1].rot.w = f32::NAN;
        // serde_json écrit « null » : le projet ne se relisait plus
        for text in [serde_json::to_string(&p).unwrap(), serde_json::to_string_pretty(&p).unwrap()] {
            assert!(text.contains("null"));
            assert!(serde_json::from_str::<Project>(&text).is_err());
            let (q, n) = parse_project(&text).expect("relu");
            assert_eq!(n, 3);
            let e = &q.ymaps[0].entities;
            assert_eq!(e[0].pos, v3(0.0, 2.0, 3.0));
            assert_eq!(e[1].scale_xy, 0.0);
        }
        // nettoyé avant d'écrire : plus de null du tout
        sanitize_project(&mut p);
        let text = serde_json::to_string(&p).unwrap();
        let (q, n) = parse_project(&text).unwrap();
        assert_eq!(n, 0);
        assert_eq!(q.ymaps[0].entities[1].scale_xy, 1.0);
        assert_eq!(q.ymaps[0].entities[1].rot, crate::math::Quat::IDENTITY);
    }

    #[test]
    fn autres_erreurs_gardees() {
        // une autre erreur n'est pas « réparée »
        assert!(parse_project("{\"name\": 3}").is_err());
        assert!(parse_project("pas du json").is_err());
    }

    #[test]
    fn enregistrement_atomique_avec_copie() {
        let d = std::env::temp_dir().join(format!("dw_files_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("p.dwproj");
        write_project_file(&f, b"version 1").unwrap();
        assert!(!d.join("p.dwproj.bak").exists());
        write_project_file(&f, b"version 2").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"version 2");
        assert_eq!(std::fs::read(d.join("p.dwproj.bak")).unwrap(), b"version 1");
        assert!(!d.join("p.dwproj.tmp").exists(), "pas de fichier temporaire laissé");
        // dossier absent : erreur, rien d'écrit
        assert!(write_project_file(&d.join("absent").join("x.dwproj"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
