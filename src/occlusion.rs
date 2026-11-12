//! Boîtes d'occlusion : le jeu ne dessine pas ce qui est entièrement caché derrière (indispensable pour une ville
//! dense). Format de CodeWalker (YmapBoxOccluder::UpdateBoxStruct 0x06002e06, lecture .ctor 0x06002e05) :
//! - iCenterX/Y/Z = round(centre × 4) et iLength/iWidth/iHeight = round(taille × 4) (16 bits, quarts de mètre) ;
//! - v = orientation × (1, 0, 0) × 0,5 ; iSinZ = round(v.x × 32767), iCosZ = round(v.y × 32767) (les noms sont
//!   inversés dans CodeWalker) ; relu par angle = atan2(iCosZ, iSinZ), orientation = rotation de cet angle autour de Z.
//! - Une carte qui en contient a le bit 5 (32) de contentFlags (YmapFile::CalcFlags 0x06002cd4, IL_0161–IL_0172).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoxOcc {
    pub pos: V3,
    /// longueur (axe x local), largeur, hauteur (m)
    pub size: V3,
    /// rotation autour de Z (radians)
    pub heading: f32,
}

/// Boîte d'occlusion d'un bâtiment : sa boîte réduite à 80 % en largeur et longueur et 90 % en hauteur, pour rester
/// à l'intérieur des murs (une boîte qui dépasse cacherait à tort ce qui est devant) — **estimé**.
const SHRINK_XY: f32 = 0.8;
const SHRINK_Z: f32 = 0.9;
/// Un bâtiment doit faire au moins 8 m de côté et 6 m de haut pour recevoir une boîte — **estimé**.
const MIN_XY: f32 = 8.0;
const MIN_Z: f32 = 6.0;
/// Au plus 300 boîtes par carte avec l'occlusion automatique (les plus grosses d'abord) — **estimé** : le jeu
/// teste chaque boîte à chaque image, trop de petites boîtes coûteraient plus qu'elles ne font gagner.
pub const AUTO_MAX_PER_MAP: usize = 300;

/// Boîte d'occlusion d'un objet dont la boîte locale est (mn, mx) : rien si trop petit.
pub fn box_for(e: &crate::model::Entity, mn: V3, mx: V3) -> Option<BoxOcc> {
    let s = (mx - mn).mulv(e.scale());
    if s.x.max(s.y) < MIN_XY || s.z < MIN_Z {
        return None;
    }
    let c = e.pos + e.rot.rotate(((mn + mx) * 0.5).mulv(e.scale()));
    let heading = e.rot.heading_deg().to_radians();
    Some(BoxOcc { pos: c, size: v3(s.x * SHRINK_XY, s.y * SHRINK_XY, s.z * SHRINK_Z), heading })
}

/// Une boîte quasi identique (même centre à 1,5 m près, même taille à 20 % près) existe déjà.
pub fn covered(list: &[BoxOcc], b: &BoxOcc) -> bool {
    list.iter().any(|o| {
        (o.pos - b.pos).len() < 1.5 && (o.size.x - b.size.x).abs() <= 0.2 * b.size.x.max(o.size.x) && (o.size.y - b.size.y).abs() <= 0.2 * b.size.y.max(o.size.y) && (o.size.z - b.size.z).abs() <= 0.2 * b.size.z.max(o.size.z)
    })
}

/// Parmi `cands`, garde les plus grosses boîtes sans doublon (avec `existing` ni entre elles), pour que la carte
/// n'ait jamais plus de `max` boîtes en tout.
pub fn pick_boxes(mut cands: Vec<BoxOcc>, existing: &[BoxOcc], max: usize) -> Vec<BoxOcc> {
    let room = max.saturating_sub(existing.len());
    cands.sort_by(|a, b| (b.size.x * b.size.y * b.size.z).total_cmp(&(a.size.x * a.size.y * a.size.z)));
    let mut out: Vec<BoxOcc> = Vec::new();
    for c in cands {
        if out.len() >= room {
            break;
        }
        if !covered(existing, &c) && !covered(&out, &c) {
            out.push(c);
        }
    }
    out
}

/// Liste XML d'une carte (CMapData.boxOccluders).
pub fn occluders_xml(list: &[BoxOcc]) -> String {
    let mut s = String::from(" <boxOccluders>\n");
    for b in list {
        let q = |v: f32| (v * 4.0).round().clamp(-32768.0, 32767.0) as i32;
        let dir = Quat::from_z(b.heading).rotate(v3(1.0, 0.0, 0.0)) * 0.5;
        let t = |v: f32| (v * 32767.0).round() as i32;
        s += &format!(
            "  <Item>\n   <iCenterX value=\"{}\" />\n   <iCenterY value=\"{}\" />\n   <iCenterZ value=\"{}\" />\n   <iCosZ value=\"{}\" />\n   <iLength value=\"{}\" />\n   <iWidth value=\"{}\" />\n   <iHeight value=\"{}\" />\n   <iSinZ value=\"{}\" />\n  </Item>\n",
            q(b.pos.x), q(b.pos.y), q(b.pos.z), t(dir.y), q(b.size.x), q(b.size.y), q(b.size.z), t(dir.x)
        );
    }
    s.push_str(" </boxOccluders>\n");
    s
}

impl DwApp {
    /// Une boîte d'occlusion dans chaque grand bâtiment sélectionné (rangée dans la carte du bâtiment).
    pub fn add_occluders_for_selection(&mut self) -> String {
        let mut add: Vec<(usize, BoxOcc)> = Vec::new();
        for id in self.selected.clone() {
            let Some(crate::model::ItemRef::Ent(yi, _)) = self.proj.find_id(id) else { continue };
            let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.ent(r)) else { continue };
            if e.mlo.is_some() {
                continue;
            }
            let (mn, mx) = e.local_box(&self.proj);
            if let Some(b) = box_for(e, mn, mx) {
                add.push((yi, b));
            }
        }
        if add.is_empty() {
            return "Choisis un ou plusieurs grands bâtiments (au moins 8 m de côté et 6 m de haut).".into();
        }
        self.push_undo();
        let n = add.len();
        for (yi, b) in add {
            self.proj.ymaps[yi].occluders.push(b);
        }
        format!("{n} boîte(s) d'occlusion ajoutée(s) (violet) : le jeu ne dessinera pas ce qui est caché derrière.")
    }

    /// Occlusion automatique : une boîte dans chaque grand bâtiment de toutes les cartes du projet (pas les props,
    /// les intérieurs, les versions lointaines ni les modèles de DragoWalker), sans doublon, les plus grosses d'abord.
    pub fn add_occluders_auto(&mut self) -> String {
        let mut plan: Vec<(usize, Vec<BoxOcc>)> = Vec::new();
        let mut candidates_total = 0;
        for (yi, y) in self.proj.ymaps.iter().enumerate().filter(|(_, y)| !y.sample) {
            let mut cands: Vec<BoxOcc> = Vec::new();
            for e in &y.entities {
                let low = e.archetype.to_lowercase();
                let detailed = matches!(e.lod_level.as_str(), "" | "LODTYPES_DEPTH_HD" | "LODTYPES_DEPTH_ORPHANHD");
                if e.mlo.is_some() || !detailed || low.starts_with("dw_") || crate::buildings::is_prop_name(&low) {
                    continue;
                }
                let (mn, mx) = e.local_box(&self.proj);
                if let Some(b) = box_for(e, mn, mx) {
                    cands.push(b);
                }
            }
            candidates_total += cands.len();
            let picked = pick_boxes(cands, &y.occluders, AUTO_MAX_PER_MAP);
            if !picked.is_empty() {
                plan.push((yi, picked));
            }
        }
        let n: usize = plan.iter().map(|p| p.1.len()).sum();
        if n == 0 {
            return if candidates_total == 0 { "Aucun grand bâtiment (8 m de côté, 6 m de haut) dans le projet.".into() } else { "Les grands bâtiments ont déjà leur boîte d'occlusion (ou la limite de la carte est atteinte).".into() };
        }
        self.push_undo();
        for (yi, boxes) in plan {
            self.proj.ymaps[yi].occluders.extend(boxes);
        }
        format!("{n} boîte(s) d'occlusion ajoutée(s) (violet), au plus {AUTO_MAX_PER_MAP} par carte : le jeu ne dessine pas ce qui est caché derrière. Ctrl+Z annule.")
    }

    /// Dans la vue : boîtes d'occlusion en violet.
    pub fn draw_occluders(&self, painter: &egui::Painter) {
        for b in self.proj.ymaps.iter().filter(|y| y.visible).flat_map(|y| y.occluders.iter()) {
            if (b.pos - self.cam.target).len() > 1500.0 {
                continue;
            }
            let q = Quat::from_z(b.heading);
            let h = b.size * 0.5;
            let mut w = [V3::ZERO; 8];
            for (i, c) in w.iter_mut().enumerate() {
                let l = v3(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z });
                *c = b.pos + q.rotate(l);
            }
            crate::render::draw_box_edges(painter, &self.cam, &w, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(190, 90, 255)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x: f32, s: f32) -> BoxOcc {
        BoxOcc { pos: v3(x, 0.0, 0.0), size: v3(s, s, s), heading: 0.0 }
    }

    #[test]
    fn automatique_sans_doublon_et_plafonnee() {
        // un bâtiment 20 × 10 × 30 m à (100, 0, 0), tourné de 0 : boîte réduite à 80 % / 90 %
        let e = crate::model::Entity::new("dt1_05_build2_h", v3(100.0, 0.0, 0.0));
        let b = box_for(&e, v3(-10.0, -5.0, 0.0), v3(10.0, 5.0, 30.0)).expect("grand bâtiment");
        assert!((b.size.x - 16.0).abs() < 1e-3 && (b.size.y - 8.0).abs() < 1e-3 && (b.size.z - 27.0).abs() < 1e-3);
        assert!((b.pos.z - 15.0).abs() < 1e-3);
        // trop petit : pas de boîte
        assert!(box_for(&e, v3(-2.0, -2.0, 0.0), v3(2.0, 2.0, 3.0)).is_none());
        // doublons : même place même taille = couvert ; plus loin = non
        assert!(covered(&[bx(0.0, 10.0)], &bx(1.0, 10.5)));
        assert!(!covered(&[bx(0.0, 10.0)], &bx(20.0, 10.0)));
        assert!(!covered(&[bx(0.0, 10.0)], &bx(0.5, 20.0)));
        // 10 candidates de tailles différentes + 1 déjà présente + 1 doublon : plafond 5 en tout, les plus grosses d'abord
        let existing = vec![bx(0.0, 50.0)];
        let mut cands: Vec<BoxOcc> = (1..=10).map(|i| bx(i as f32 * 100.0, 10.0 + i as f32)).collect();
        cands.push(bx(0.2, 50.0)); // doublon de l'existante
        cands.push(bx(300.0, 13.0)); // doublon d'une autre candidate
        let picked = pick_boxes(cands, &existing, 5);
        assert_eq!(picked.len(), 4, "5 en tout moins l'existante");
        let mut sizes: Vec<f32> = picked.iter().map(|b| b.size.x).collect();
        sizes.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(sizes, vec![20.0, 19.0, 18.0, 17.0]);
        // carte déjà pleine : rien
        assert!(pick_boxes(vec![bx(500.0, 30.0)], &vec![bx(0.0, 5.0); 3], 3).is_empty());
    }

    #[test]
    fn encoding_matches_codewalker() {
        // cap 0 : v = (0,5 ; 0) → iSinZ = 16384, iCosZ = 0 ; relu atan2(iCosZ, iSinZ) = 0
        let s = occluders_xml(&[BoxOcc { pos: v3(10.0, -2.5, 3.0), size: v3(8.0, 4.0, 6.0), heading: 0.0 }]);
        assert!(s.contains("<iCenterX value=\"40\" />") && s.contains("<iCenterY value=\"-10\" />") && s.contains("<iLength value=\"32\" />"));
        assert!(s.contains("<iSinZ value=\"16384\" />") && s.contains("<iCosZ value=\"0\" />"));
        // cap 90° : v = (0 ; 0,5) → iSinZ = 0, iCosZ = 16384
        let s = occluders_xml(&[BoxOcc { pos: V3::ZERO, size: v3(1.0, 1.0, 1.0), heading: std::f32::consts::FRAC_PI_2 }]);
        // (0,5 × 32767 = 16383,5 : l'arrondi peut donner 16383 selon l'imprécision de la rotation, comme dans CodeWalker)
        assert!(s.contains("<iSinZ value=\"0\" />") && (s.contains("<iCosZ value=\"16384\" />") || s.contains("<iCosZ value=\"16383\" />")));
    }
}
