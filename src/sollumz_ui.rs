//! Panneau « Mes modèles (Blender / Sollumz) » : dossier d'échange surveillé, modèles personnels
//! avec aperçu et pose dans la carte, ouverture de Blender, envoi de la sélection vers Sollumz.

use crate::app::{DwApp, Tool};
use crate::gta::loader::Loaded;
use crate::model::Ymap;
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};

const SOLLUMZ_PAGE: &str = "https://github.com/Sollumz/Sollumz/releases";

impl DwApp {
    /// Surveille le dossier d'échange (toutes les 2 s) et envoie les modèles à la carte graphique.
    pub fn sollumz_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.sollumz.last_scan > 2.0 {
            self.sollumz.last_scan = now;
            if self.sollumz.refresh(false) {
                crate::catalog::set_custom_boxes(self.sollumz.boxes());
                // l'aperçu sera recalculé
                self.game.preview_for = 0;
                if !self.sollumz.models.is_empty() {
                    self.status = format!("Modèles personnels (Sollumz) chargés : {}.", self.sollumz.models.len());
                }
            }
        }
        if let Some(gpu) = &self.game.gpu {
            if self.sollumz_sent != self.sollumz.generation {
                if let Ok(mut g) = gpu.lock() {
                    for m in &self.sollumz.models {
                        g.add_custom(Loaded {
                            arch: crate::gta::crypto::joaat(&m.name),
                            model: Some(m.model.clone()),
                            part_keys: m.part_keys.clone(),
                            textures: m.textures.clone(),
                        });
                    }
                }
                self.sollumz_sent = self.sollumz.generation;
            }
        }
    }

    /// Copie les fichiers des modèles personnels utilisés dans le projet vers le dossier d'export.
    pub fn copy_custom_models(&self, stream: &Path) -> usize {
        let used: std::collections::HashSet<String> = self
            .proj
            .ymaps
            .iter()
            .flat_map(|y| y.entities.iter().map(|e| e.archetype.to_lowercase()))
            .chain(self.proj.ytyps.iter().flat_map(|t| t.mlos.iter().flat_map(|m| m.entities.iter().map(|e| e.archetype.to_lowercase()))))
            .collect();
        let mut n = 0;
        for m in self.sollumz.models.iter().filter(|m| used.contains(&m.name)) {
            if let Some(name) = m.file.file_name() {
                if std::fs::copy(&m.file, stream.join(name)).is_ok() {
                    n += 1;
                }
            }
            // textures du modèle (dossier du même nom ou même dossier)
            let dir = m.file.parent().unwrap_or(Path::new("."));
            for d in [dir.join(&m.name), dir.to_path_buf()] {
                if let Ok(rd) = std::fs::read_dir(&d) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.extension().map(|x| x.eq_ignore_ascii_case("dds")).unwrap_or(false) {
                            let sub = stream.join(&m.name);
                            let _ = std::fs::create_dir_all(&sub);
                            let _ = std::fs::copy(&p, sub.join(e.file_name()));
                        }
                    }
                }
            }
        }
        // types (.ytyp.xml) du dossier d'échange
        if let Ok(rd) = std::fs::read_dir(&self.sollumz.dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.to_string_lossy().to_lowercase().ends_with(".ytyp.xml") && std::fs::copy(&p, stream.join(e.file_name())).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    /// Exporte le modèle d'un prop du jeu (formes + textures) vers le dossier d'échange et l'ouvre dans Blender.
    pub fn edit_in_blender(&mut self, name: &str) {
        let Some(w) = self.game.world.as_ref() else {
            self.status = "Charge d'abord le jeu (onglet Jeu).".into();
            return;
        };
        let arch = crate::render::arch_hash(name);
        let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
        let l = w.model_source().load(arch, &claimed, &mut Vec::new());
        let Some(model) = l.model else {
            self.status = format!("Modèle de « {name} » introuvable dans le jeu.");
            return;
        };
        let dir = PathBuf::from(&self.sollumz.dir);
        let _ = std::fs::create_dir_all(&dir);
        match crate::sollumz::write_ydr_xml(&dir, &name.to_lowercase(), &model, &l.part_keys, &l.textures) {
            Ok(path) => {
                let msg = match &self.sollumz.blender {
                    Some(b) => match crate::sollumz::open_in_blender(b, &path) {
                        Ok(_) => "Blender s'ouvre avec le modèle importé par Sollumz. Modifie-le puis exporte-le avec Sollumz dans ce même dossier : DragoWalker l'affichera à la place de l'original.".to_string(),
                        Err(e) => e,
                    },
                    None => "Blender introuvable : ouvre ce fichier avec Sollumz.".into(),
                };
                self.status = format!("{} écrit. {msg}", path.display());
            }
            Err(e) => self.status = format!("Écriture impossible : {e}"),
        }
    }

    /// Écrit la sélection en .ymap.xml dans le dossier d'échange, pour l'importer dans Blender avec Sollumz.
    fn send_selection_to_sollumz(&mut self) {
        let mut y = Ymap::new("dragowalker_selection");
        for id in &self.selected {
            if let Some(e) = self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r)) {
                y.entities.push(e);
            }
        }
        if y.entities.is_empty() {
            self.status = "Sélectionne d'abord des objets (clic, rectangle…).".into();
            return;
        }
        y.calc_flags();
        let dir = PathBuf::from(&self.sollumz.dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dragowalker_selection.ymap.xml");
        match std::fs::write(&path, crate::xmlio::write_ymap(&y, &self.proj)) {
            Ok(_) => {
                let n = y.entities.len();
                self.status = match self.sollumz.blender.clone().map(|b| crate::sollumz::open_in_blender(&b, &path)) {
                    Some(Ok(_)) => format!("{n} objet(s) envoyés : Blender s'ouvre et Sollumz importe {}.", path.display()),
                    _ => format!("{n} objet(s) envoyés vers {} : dans Blender, Sollumz » Importer, puis choisis ce fichier.", path.display()),
                };
            }
            Err(e) => self.status = format!("Écriture impossible : {e}"),
        }
    }

    pub fn sollumz_panel(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("Mes modèles (Blender + Sollumz)").strong()).id_salt("sollumz").default_open(true).show(ui, |ui| {
            ui.label(RichText::new("Crée ou modifie tes objets dans Blender avec le module Sollumz, exporte-les en XML dans le dossier d'échange : ils apparaissent ici tout seuls, prêts à être posés sur la carte.").weak().small());
            match &self.sollumz.blender {
                Some(p) => ui.label(RichText::new(format!("✔ Blender trouvé : {}", p.display())).small().color(Color32::from_rgb(140, 210, 140))),
                None => ui.label(RichText::new("Blender introuvable sur cet ordinateur (blender.org).").small().color(Color32::from_rgb(255, 170, 120))),
            };
            match &self.sollumz.sollumz {
                Some(v) => {
                    ui.label(RichText::new(format!("✔ Sollumz installé dans {v}")).small().color(Color32::from_rgb(140, 210, 140)));
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Sollumz n'est pas installé dans Blender.").small().color(Color32::from_rgb(255, 170, 120)));
                        if ui.small_button("Obtenir Sollumz").on_hover_text(SOLLUMZ_PAGE).clicked() {
                            let _ = std::process::Command::new("explorer").arg(SOLLUMZ_PAGE).spawn();
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                ui.label("Dossier d'échange");
                let r = ui.add(egui::TextEdit::singleline(&mut self.sollumz.dir).desired_width(170.0));
                if r.lost_focus() {
                    crate::app::save_setting("sollumz_dir", serde_json::json!(self.sollumz.dir));
                    self.sollumz.refresh(true);
                    crate::catalog::set_custom_boxes(self.sollumz.boxes());
                }
                if ui.small_button("…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.sollumz.dir = p.to_string_lossy().to_string();
                        crate::app::save_setting("sollumz_dir", serde_json::json!(self.sollumz.dir));
                        self.sollumz.refresh(true);
                        crate::catalog::set_custom_boxes(self.sollumz.boxes());
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Ouvrir le dossier").clicked() {
                    let _ = std::fs::create_dir_all(&self.sollumz.dir);
                    let _ = std::process::Command::new("explorer").arg(&self.sollumz.dir).spawn();
                }
                if let Some(b) = self.sollumz.blender.clone() {
                    if ui.small_button("Ouvrir Blender").clicked() {
                        match std::process::Command::new(&b).spawn() {
                            Ok(_) => self.status = "Blender s'ouvre… Exporte avec Sollumz (format XML de CodeWalker) dans le dossier d'échange.".into(),
                            Err(e) => self.status = format!("Impossible d'ouvrir Blender : {e}"),
                        }
                    }
                }
                if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Envoyer la sélection vers Sollumz").small()).on_hover_text("Écrit les objets sélectionnés en .ymap.xml dans le dossier d'échange").clicked() {
                    self.send_selection_to_sollumz();
                }
                if ui.small_button("⟳ Relire").clicked() {
                    self.sollumz.refresh(true);
                    crate::catalog::set_custom_boxes(self.sollumz.boxes());
                    self.game.preview_for = 0;
                }
            });
            if self.sollumz.models.is_empty() {
                ui.label(RichText::new("Aucun modèle dans le dossier pour l'instant (fichiers .ydr.xml + textures .dds).").weak().small());
            } else {
                let mut picked = None;
                egui::ScrollArea::vertical().max_height(140.0).id_salt("sollumz_list").show(ui, |ui| {
                    for m in &self.sollumz.models {
                        let sel = self.place_arch.eq_ignore_ascii_case(&m.name);
                        let size = m.bb_max - m.bb_min;
                        let mut txt = format!("{}  ·  {:.1} × {:.1} × {:.1} m  ·  {} triangles", m.name, size.x, size.y, size.z, m.tris);
                        if !m.missing_textures.is_empty() {
                            txt += &format!("  ·  {} texture(s) manquante(s)", m.missing_textures.len());
                        }
                        if ui.selectable_label(sel, RichText::new(txt).size(12.5)).on_hover_text(m.file.display().to_string()).clicked() {
                            picked = Some(m.name.clone());
                        }
                    }
                });
                if let Some(n) = picked {
                    self.place_arch = n;
                    self.tool = Tool::Place;
                    self.game.preview_for = 0;
                }
                // aperçu du modèle personnel choisi
                if let Some(m) = self.sollumz.find(&self.place_arch) {
                    let h = crate::gta::crypto::joaat(&m.name);
                    if self.game.preview_for != h {
                        self.game.preview = Some(crate::sollumz::preview_mesh(m));
                        self.game.preview_for = h;
                    }
                    self.preview_widget(ui);
                }
            }
            for e in &self.sollumz.errors {
                ui.label(RichText::new(e).small().color(Color32::from_rgb(255, 140, 110)));
            }
        });
    }
}
