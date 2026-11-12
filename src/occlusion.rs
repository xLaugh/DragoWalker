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
            let s = (mx - mn).mulv(e.scale());
            if s.x.max(s.y) < MIN_XY || s.z < MIN_Z {
                continue;
            }
            let c = e.pos + e.rot.rotate(((mn + mx) * 0.5).mulv(e.scale()));
            let heading = e.rot.heading_deg().to_radians();
            add.push((yi, BoxOcc { pos: c, size: v3(s.x * SHRINK_XY, s.y * SHRINK_XY, s.z * SHRINK_Z), heading }));
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
            crate::render::draw_box_edges(painter, &self.cam, &w, egui::Stroke::new(1.5, egui::Color32::from_rgb(190, 90, 255)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
