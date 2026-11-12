//! Actions de l'assistant IA appliquées au projet, résumé de la scène et conversation.
//! (Déplacé de app.rs sans modification.)

use crate::ai::{self, Action, AiReply, ChatLine};
use crate::app::*;
use crate::math::v3;
use std::collections::HashSet;

impl DwApp {
    /// Exécute une action de l'assistant.
    pub fn run_action(&mut self, a: &Action) -> String {
        let c = v3(self.cam.target.x, self.cam.target.y, self.ground_z);
        match a {
            Action::Place { archetype, count, pattern, size } => {
                self.push_undo();
                let n = (*count).clamp(1, 500);
                let mut ids = Vec::new();
                let (fwd, right) = self.cam.flat_axes();
                for i in 0..n {
                    let fi = i as f32;
                    let p = match pattern.as_str() {
                        "ligne" => c + right * ((fi - (n as f32 - 1.0) * 0.5) * size),
                        "cercle" => {
                            let a = fi / n as f32 * std::f32::consts::TAU;
                            c + v3(a.cos() * size, a.sin() * size, 0.0)
                        }
                        "grille" => {
                            let side = (n as f32).sqrt().ceil() as u32;
                            let gx = (i % side) as f32 - (side as f32 - 1.0) * 0.5;
                            let gy = (i / side) as f32 - (side as f32 - 1.0) * 0.5;
                            c + right * (gx * size) + fwd * (gy * size)
                        }
                        "aleatoire" => {
                            let a = self.rng.range(0.0, std::f32::consts::TAU);
                            let r = size * self.rng.f().sqrt();
                            c + v3(a.cos() * r, a.sin() * r, 0.0)
                        }
                        _ => c,
                    };
                    let heading = match pattern.as_str() {
                        "aleatoire" => Some(self.rng.range(0.0, 360.0)),
                        "cercle" => Some((fi / n as f32 * 360.0 + 90.0) % 360.0),
                        _ => Some(0.0),
                    };
                    ids.push(self.place_prop(archetype, p, heading));
                }
                self.set_selection(ids);
                format!("{n} × {archetype} posé(s).")
            }
            Action::DeleteRadius { radius, filter } => {
                let ids: HashSet<u64> = self.items_in_radius(c, *radius, filter, true).into_iter().collect();
                if ids.is_empty() {
                    return "Rien à supprimer dans cette zone.".into();
                }
                self.push_undo();
                let n = self.proj.delete_ids(&ids);
                self.clean_selection();
                format!("{n} objet(s) supprimé(s) dans un rayon de {radius} m.")
            }
            Action::DeleteSelection => {
                let n = self.selected.len();
                self.delete_selected();
                format!("{n} objet(s) supprimé(s).")
            }
            Action::Select { filter, radius } => {
                let n = self.select_filtered(filter, *radius);
                format!("{n} objet(s) sélectionné(s).")
            }
            Action::ClearSelection => {
                self.set_selection(vec![]);
                "Sélection vidée.".into()
            }
            Action::MoveSelection { dx, dy, dz } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                self.move_selected(v3(*dx, *dy, *dz));
                "Sélection déplacée.".into()
            }
            Action::RotateSelection { degrees } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                self.rotate_selected(*degrees);
                format!("Sélection tournée de {degrees}°.")
            }
            Action::ScaleSelection { factor } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                for id in self.selected.clone() {
                    if let Some(r) = self.proj.find_id(id) {
                        if let Some(e) = self.proj.ent_mut(r) {
                            e.scale_xy = (e.scale_xy * factor).clamp(0.05, 50.0);
                            e.scale_z = (e.scale_z * factor).clamp(0.05, 50.0);
                        }
                    }
                }
                format!("Taille ×{factor}.")
            }
            Action::DuplicateSelection { dx, dy } => {
                self.duplicate_selected(v3(*dx, *dy, 0.0));
                "Sélection dupliquée.".into()
            }
            Action::CreateMlo { name, cols, rows, width, depth, height } => {
                self.create_mlo(name, (*cols).clamp(1, 8), (*rows).clamp(1, 8), width.clamp(2.0, 60.0), depth.clamp(2.0, 60.0), height.clamp(2.0, 20.0));
                self.status.clone()
            }
            Action::FurnishMlo { style } => self.furnish_mlo(style),
            Action::Goto { x, y } => {
                self.cam.target = v3(*x, *y, self.ground_z);
                format!("Vue centrée sur ({x}, {y}).")
            }
            Action::Undo => {
                self.do_undo();
                "Annulé.".into()
            }
        }
    }

    pub fn scene_summary(&self) -> String {
        let mut s = format!(
            "Centre de la vue : ({:.1}, {:.1}, {:.1}). Sol à z = {:.1}.\nSélection : {} objet(s).\n",
            self.cam.target.x, self.cam.target.y, self.cam.target.z, self.ground_z, self.selected.len()
        );
        for id in self.selected.iter().take(15) {
            if let Some(e) = self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r)) {
                s += &format!("  - {} à ({:.1}, {:.1}, {:.1})\n", e.archetype, e.pos.x, e.pos.y, e.pos.z);
            }
        }
        for y in &self.proj.ymaps {
            s += &format!("Ymap {} : {} objets{}\n", y.name, y.entities.len(), if y.sample { " (carte d'exemple)" } else { "" });
        }
        for t in &self.proj.ytyps {
            for m in &t.mlos {
                s += &format!("Intérieur {} : {} pièces, {} portails, {} objets\n", m.name, m.rooms.len().saturating_sub(1), m.portals.len(), m.entities.len());
            }
        }
        let near = self.items_in_radius(self.cam.target, 60.0, "", true);
        s += &format!("Objets à moins de 60 m du centre : {}\n", near.len());
        s
    }

    pub fn send_chat(&mut self) {
        let text = self.chat_input.trim().to_string();
        if text.is_empty() || self.pending.is_some() {
            return;
        }
        self.chat_input.clear();
        self.chat.push(ChatLine { from_user: true, text: text.clone() });
        if self.ai_provider != ai::Provider::Local {
            let cfg = ai::AiConfig {
                provider: self.ai_provider,
                claude_key: self.api_key.trim().to_string(),
                claude_code: self.claude_code.clone(),
                cc_model: self.cc_model.clone(),
                oa_base: self.oa_base.clone(),
                oa_key: self.oa_key.clone(),
                oa_model: self.oa_model.clone(),
                cli_cmd: self.cli_cmd.clone(),
            };
            self.pending = Some(ai::ask(cfg, self.chat.clone(), self.scene_summary()));
        } else {
            let r = ai::local_parse(&text);
            self.apply_reply(r);
        }
    }

    pub fn apply_reply(&mut self, r: AiReply) {
        if std::env::var("DW_TEST_AI").is_ok() {
            println!("réponse IA : {} | actions {:?}", r.message, r.actions);
        }
        let mut out = r.message;
        for a in &r.actions {
            let res = self.run_action(a);
            out.push_str(&format!("\n✔ {res}"));
        }
        self.status = out.lines().last().unwrap_or("").trim_start_matches("✔ ").to_string();
        self.chat.push(ChatLine { from_user: false, text: out });
    }
}
