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
/// Au plus 300 boîtes par carte avec l'occlusion automatique (les plus grosses d'abord). Comparé au jeu (206 cartes
/// *_occl relues le 18 nov. 2026) : 91 boîtes par carte en moyenne, 603 au plus (vb_occl_01), et jusqu'à 22 modèles
/// d'occlusion : 300 reste dans ce que le jeu charge lui-même.
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

/// Modèle d'occlusion (CMapData.occludeModels), comme ceux des cartes *_occl du jeu (relues en XML par CodeWalker,
/// 18 nov. 2026) : sommets en coordonnées du monde (float32 x, y, z à la suite), puis les triangles en numéros de
/// sommets sur un octet (256 sommets au plus) ; dataSize = octets des sommets + 3 × triangles ; numTris porte le
/// drapeau 0x8000 (toutes les valeurs du jeu sont 32768 + nombre de triangles) ; flags 0.
pub struct OccModel {
    pub verts: Vec<V3>,
    pub tris: Vec<[u8; 3]>,
}

/// Volume fermé d'un bâtiment de l'Atelier : son emprise (repère du bâtiment, sens inverse des aiguilles d'une montre)
/// rentrée de `inset` m, de `z0` à `z1`, placée à `pos` / `rot`. None si l'emprise a trop de côtés (plus de 128).
pub fn extruded_model(foot: &[[f32; 2]], inset: f32, z0: f32, z1: f32, pos: V3, rot: Quat) -> Option<OccModel> {
    let n = foot.len();
    if !(3..=128).contains(&n) {
        return None;
    }
    // rentrée : chaque sommet poussé vers l'intérieur le long de la bissectrice (côtés décalés de `inset`)
    let p = |i: usize| v3(foot[i % n][0], foot[i % n][1], 0.0);
    let mut ring = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b, c) = (p(i + n - 1), p(i), p(i + 1));
        let (d1, d2) = ((b - a).norm(), (c - b).norm());
        // normales vers l'intérieur (polygone dans le sens inverse des aiguilles d'une montre : à gauche)
        let (n1, n2) = (v3(-d1.y, d1.x, 0.0), v3(-d2.y, d2.x, 0.0));
        let m = (n1 + n2).norm();
        let cosh = m.dot(n1).max(0.3);
        ring.push(b + m * (inset / cosh));
    }
    let mut verts = Vec::with_capacity(2 * n);
    for z in [z0, z1] {
        for q in &ring {
            verts.push(pos + rot.rotate(v3(q.x, q.y, z)));
        }
    }
    let mut tris = Vec::new();
    // murs (faces vers l'extérieur)
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, c, d) = (i as u8, j as u8, (n + j) as u8, (n + i) as u8);
        tris.push([a, b, c]);
        tris.push([a, c, d]);
    }
    // dessus et dessous : découpage en oreilles (emprise en L, en U…)
    let flat: Vec<[f32; 2]> = ring.iter().map(|q| [q.x, q.y]).collect();
    for t in ear_clip(&flat) {
        tris.push([(n + t[0]) as u8, (n + t[1]) as u8, (n + t[2]) as u8]);
        tris.push([t[0] as u8, t[2] as u8, t[1] as u8]);
    }
    Some(OccModel { verts, tris })
}

/// Triangles d'un polygone simple dans le sens inverse des aiguilles d'une montre (découpage en oreilles).
pub fn ear_clip(p: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..p.len()).collect();
    let mut out = Vec::new();
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let inside = |q: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]| cross(a, b, q) > 0.0 && cross(b, c, q) > 0.0 && cross(c, a, q) > 0.0;
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let m = idx.len();
        let mut cut = None;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (p[ia], p[ib], p[ic]);
            if cross(a, b, c) <= 1e-6 {
                continue; // angle rentrant
            }
            if idx.iter().any(|&o| o != ia && o != ib && o != ic && inside(p[o], a, b, c)) {
                continue;
            }
            cut = Some(k);
            out.push([ia, ib, ic]);
            break;
        }
        match cut {
            Some(k) => {
                idx.remove(k);
            }
            None => break, // polygone dégénéré
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// Liste XML (CMapData.occludeModels).
pub fn occlude_models_xml(list: &[OccModel]) -> String {
    use std::fmt::Write;
    let mut s = String::from(" <occludeModels>\n");
    for m in list {
        let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
        let mut bytes = Vec::with_capacity(m.verts.len() * 12 + m.tris.len() * 3);
        for v in &m.verts {
            mn = mn.min(*v);
            mx = mx.max(*v);
            for c in [v.x, v.y, v.z] {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        let nv = bytes.len();
        for t in &m.tris {
            bytes.extend_from_slice(t);
        }
        let mut hex = String::new();
        for (i, b) in bytes.iter().enumerate() {
            if i % 32 == 0 {
                hex += "\n    ";
            } else {
                hex.push(' ');
            }
            let _ = write!(hex, "{b:02X}");
        }
        let _ = write!(
            s,
            "  <Item>\n   <bmin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bmax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <dataSize value=\"{}\" />\n   <verts>{hex}\n   </verts>\n   <numVertsInBytes value=\"{nv}\" />\n   <numTris value=\"{}\" />\n   <flags value=\"0\" />\n  </Item>\n",
            mn.x,
            mn.y,
            mn.z,
            mx.x,
            mx.y,
            mx.z,
            bytes.len(),
            0x8000 + m.tris.len()
        );
    }
    s.push_str(" </occludeModels>\n");
    s
}

/// Rentrée des modèles d'occlusion des bâtiments (m), sous les murs : 30 cm — **estimé**.
pub const MODEL_INSET: f32 = 0.3;

/// Modèles d'occlusion des bâtiments de l'Atelier placés dans une carte (un par placement, sauf les versions
/// lointaines et les petits bâtiments).
pub fn house_models(y: &crate::model::Ymap, proj: &crate::model::Project) -> Vec<OccModel> {
    let mut out = Vec::new();
    for e in y.entities.iter().filter(|e| e.mlo.is_none()) {
        let Some(h) = proj.houses.iter().find(|h| h.name.eq_ignore_ascii_case(&e.archetype)) else { continue };
        let height = h.floors as f32 * h.floor_h;
        let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
        for q in &h.foot {
            mn = [mn[0].min(q[0]), mn[1].min(q[1])];
            mx = [mx[0].max(q[0]), mx[1].max(q[1])];
        }
        if (mx[0] - mn[0]).max(mx[1] - mn[1]) < MIN_XY || height < MIN_Z {
            continue;
        }
        if let Some(m) = extruded_model(&h.foot, MODEL_INSET, 0.2, height * SHRINK_Z, e.pos, e.rot) {
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
mod tests_models {
    use super::*;

    #[test]
    fn modele_en_l() {
        // emprise en L de 20 × 20 m (6 côtés)
        let foot = [[0.0, 0.0], [20.0, 0.0], [20.0, 8.0], [8.0, 8.0], [8.0, 20.0], [0.0, 20.0]];
        let m = extruded_model(&foot, 0.3, 0.2, 9.0, v3(100.0, 200.0, 10.0), Quat::IDENTITY).unwrap();
        assert_eq!(m.verts.len(), 12);
        // 12 triangles de murs + 2 × 4 pour le dessus et le dessous
        assert_eq!(m.tris.len(), 20);
        // rentré : le coin (0, 0) passe à (0,3 ; 0,3)
        assert!((m.verts[0].x - 100.3).abs() < 1e-3 && (m.verts[0].y - 200.3).abs() < 1e-3);
        let x = occlude_models_xml(&[m]);
        assert!(x.contains("<numVertsInBytes value=\"144\" />") && x.contains("<dataSize value=\"204\" />") && x.contains("<numTris value=\"32788\" />"));
        // aire du dessus = aire du L (découpage en oreilles complet)
        let foot2: Vec<[f32; 2]> = foot.to_vec();
        let area: f32 = ear_clip(&foot2).iter().map(|t| {
            let (a, b, c) = (foot2[t[0]], foot2[t[1]], foot2[t[2]]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) * 0.5
        }).sum();
        assert!((area - 256.0).abs() < 1e-3, "{area}");
    }
}

#[cfg(test)]
mod tests_cw {
    use super::*;

    /// Conversion réelle (DW_CW_DIR) : `cargo test -- --ignored occlusion_cw --nocapture`.
    #[test]
    #[ignore]
    fn occlusion_cw() {
        let dll = crate::cwbridge::dll(&std::env::var("DW_CW_DIR").unwrap_or_default()).expect("CodeWalker");
        let dir = std::env::temp_dir().join("dw_test_occl_cw");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let foot = [[0.0, 0.0], [20.0, 0.0], [20.0, 8.0], [8.0, 8.0], [8.0, 20.0], [0.0, 20.0]];
        let m = extruded_model(&foot, 0.3, 0.2, 9.0, v3(100.0, 200.0, 10.0), Quat::IDENTITY).unwrap();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>dw_occl_test</name>\n <parent />\n <flags value=\"0\" />\n <contentFlags value=\"32\" />\n <streamingExtentsMin x=\"0\" y=\"0\" z=\"0\" />\n <streamingExtentsMax x=\"300\" y=\"400\" z=\"100\" />\n <entitiesExtentsMin x=\"100\" y=\"200\" z=\"10\" />\n <entitiesExtentsMax x=\"120\" y=\"220\" z=\"20\" />\n <entities />\n <containerLods />\n <boxOccluders />\n{} <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>dw_occl_test</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n</CMapData>\n", occlude_models_xml(&[m]));
        std::fs::write(dir.join("dw_occl_test.ymap.xml"), xml).unwrap();
        let (n, errs) = crate::cwbridge::to_binary(&dll, &dir).unwrap();
        println!("convertis {n}, erreurs {errs:?}");
        assert!(errs.is_empty() && dir.join("dw_occl_test.ymap").is_file());
        let back = dir.join("back.xml");
        crate::cwbridge::to_xml(&dll, &[(dir.join("dw_occl_test.ymap"), back.clone())]).unwrap();
        let b = std::fs::read_to_string(back).unwrap();
        let i = b.find("<occludeModels").unwrap();
        println!("{}", &b[i..i + 500]);
        assert!(b.contains("<numTris value=\"32788\" />") && b.contains("<numVertsInBytes value=\"144\" />"));
    }
}
