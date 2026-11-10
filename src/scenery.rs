//! Eau et effets de particules du créateur de terrain.
//!
//! Eau : des zones rectangulaires comme les WaterQuads du jeu (water.xml : minX, maxX, minY, maxY, z) ; dans
//! la vue elles sont dessinées par la même eau animée que la mer ; à l'export, DragoWalker écrit un water.xml
//! complet (l'eau du jeu + les nouvelles zones, pour ne pas perdre l'océan) chargé par un petit script client
//! (native FiveM LoadWaterFromPath).
//!
//! Effets : les effets d'ambiance que le jeu pose lui-même sur ses objets (CExtensionDefParticleEffect des
//! .ytyp : amb_barrel_fire, amb_steam_vent_round, amb_fly_swarm…), relevés dans les fichiers du jeu avec leur
//! échelle la plus courante. À l'export, chaque effet utilisé devient un type d'objet (dw_fx_…) qui porte
//! cette extension, avec un tout petit modèle invisible, placé là où on l'a mis.

use crate::app::DwApp;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaterZone {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub z: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Emitter {
    pub pos: V3,
    pub fx: String,
    pub scale: f32,
}

/// Effet d'ambiance : nom dans le jeu, nom affiché, catégorie, échelle la plus courante dans les .ytyp du jeu.
pub struct Fx {
    pub name: &'static str,
    pub label: &'static str,
    pub cat: &'static str,
    pub scale: f32,
}

const fn fx(name: &'static str, label: &'static str, cat: &'static str, scale: f32) -> Fx {
    Fx { name, label, cat, scale }
}

/// Effets relevés dans les .ytyp du jeu (sonde DW_YTYP_FX, puis XML par CodeWalker) ; tous de type 0
/// (ambiant), probabilité 100, drapeaux 0, couleur 0xFFFFFFFF dans la grande majorité des cas.
pub const FX: [Fx; 30] = [
    fx("amb_beach_campfire", "Feu de camp", "Feu et fumée", 1.0),
    fx("amb_barrel_fire", "Feu de baril", "Feu et fumée", 0.1),
    fx("amb_fire_ring", "Cercle de feu", "Feu et fumée", 1.0),
    fx("amb_fbi_fire_small", "Petit incendie", "Feu et fumée", 1.0),
    fx("amb_fbi_fire_large", "Grand incendie", "Feu et fumée", 1.0),
    fx("amb_int_fireplace_sml", "Feu de cheminée", "Feu et fumée", 1.0),
    fx("amb_smoke_factory", "Fumée d'usine", "Feu et fumée", 0.8),
    fx("amb_smoke_gasworks", "Fumée de raffinerie", "Feu et fumée", 1.0),
    fx("amb_smoke_scrapyard", "Fumée de casse", "Feu et fumée", 1.0),
    fx("amb_generator_smoke", "Fumée de générateur", "Feu et fumée", 1.0),
    fx("amb_fbi_smoke_linger_hvy", "Fumée épaisse", "Feu et fumée", 1.0),
    fx("amb_steam_vent_round", "Vapeur (bouche ronde)", "Vapeur et brume", 1.0),
    fx("amb_steam_vent_open_lgt", "Vapeur légère", "Vapeur et brume", 1.0),
    fx("amb_vent_haze_lg", "Air chaud", "Vapeur et brume", 1.0),
    fx("amb_cold_air_vent", "Air froid", "Vapeur et brume", 1.0),
    fx("amb_dry_ice_area", "Fumée de scène", "Vapeur et brume", 1.0),
    fx("amb_river_mist_gen", "Brume de rivière", "Vapeur et brume", 1.0),
    fx("amb_fountain_double", "Fontaine", "Eau", 1.0),
    fx("amb_int_waterfall_splash", "Éclaboussures de cascade", "Eau", 1.0),
    fx("amb_water_roof_drips", "Gouttes du toit", "Eau", 1.0),
    fx("amb_drain_water", "Écoulement d'eau", "Eau", 1.0),
    fx("amb_sprinkler_golf", "Arroseur", "Eau", 1.0),
    fx("amb_falling_leaves_l", "Feuilles qui tombent", "Nature", 1.0),
    fx("amb_falling_leaves_palm", "Palmes qui tombent", "Nature", 0.5),
    fx("amb_butterflys", "Papillons", "Nature", 1.0),
    fx("amb_fly_swarm", "Mouches", "Nature", 1.0),
    fx("amb_wind_dust_swirl", "Tourbillon de poussière", "Nature", 1.0),
    fx("amb_wind_leaves", "Feuilles au vent", "Nature", 1.0),
    fx("amb_electric_crackle", "Grésillement électrique", "Électricité", 1.0),
    fx("amb_sparking_wires", "Câbles qui crépitent", "Électricité", 1.0),
];

fn fx_color(cat: &str) -> Color32 {
    match cat {
        "Feu et fumée" => Color32::from_rgb(255, 120, 40),
        "Vapeur et brume" => Color32::from_rgb(220, 220, 230),
        "Eau" => Color32::from_rgb(70, 160, 255),
        "Nature" => Color32::from_rgb(120, 220, 90),
        _ => Color32::from_rgb(255, 230, 80),
    }
}

/// Nom du type d'objet exporté pour un effet.
pub fn fx_arch(name: &str) -> String {
    format!("dw_fx_{}", name.trim_start_matches("amb_"))
}

impl DwApp {
    /// Outil Eau : glisser un rectangle ; niveau = hauteur du point de départ + 0,2 m ; si « creuser », le sol
    /// est descendu sous l'eau à l'intérieur (raccord en pente douce sur 4 m).
    pub fn water_tool(&mut self, resp: &egui::Response, g: Option<V3>) {
        use eframe::egui::PointerButton;
        if resp.drag_started_by(PointerButton::Primary) {
            self.terrain.water_start = g;
        }
        if resp.drag_stopped() {
            let (Some(a), Some(b)) = (self.terrain.water_start.take(), g) else { return };
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
                return;
            }
            self.push_undo();
            let z = a.z + 0.2;
            let depth = self.terrain.water_depth;
            if let Some(t) = self.proj.terrain.as_mut() {
                if self.terrain.water_dig {
                    for j in 0..t.ny {
                        for i in 0..t.nx {
                            let p = v3(t.origin.x + i as f32 * t.cell, t.origin.y + j as f32 * t.cell, 0.0);
                            // distance au bord (négative dedans)
                            let dx = (x0 - p.x).max(p.x - x1);
                            let dy = (y0 - p.y).max(p.y - y1);
                            let d = dx.max(dy);
                            if d > 4.0 {
                                continue;
                            }
                            let k = if d <= 0.0 { (-d / 4.0).min(1.0) } else { 0.0 };
                            let target = z - 0.3 - depth * k;
                            let h = &mut t.h[j * t.nx + i];
                            if *h > target {
                                *h = if d <= 0.0 { target } else { *h + (target - *h) * (1.0 - d / 4.0) * 0.5 };
                            }
                        }
                    }
                }
                t.waters.push(WaterZone { min: [x0, y0], max: [x1, y1], z });
            }
            self.terrain.dirty = true;
            self.status = format!("Eau ajoutée : {:.0} × {:.0} m à {:.1} m de haut.", x1 - x0, y1 - y0, z);
        }
    }

    /// Outil Effets : clic = poser l'effet choisi ; Ctrl + clic = retirer l'effet le plus proche (5 m).
    pub fn fx_tool(&mut self, resp: &egui::Response, g: Option<V3>, ctrl: bool) {
        if !resp.clicked_by(eframe::egui::PointerButton::Primary) {
            return;
        }
        let Some(p) = g else { return };
        self.push_undo();
        let f = &FX[self.terrain.fx.min(FX.len() - 1)];
        let scale = self.terrain.fx_scale * f.scale;
        let Some(t) = self.proj.terrain.as_mut() else { return };
        if ctrl {
            if let Some((i, _)) = t.emitters.iter().enumerate().map(|(i, e)| (i, (e.pos - p).len())).filter(|x| x.1 < 5.0).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) {
                let e = t.emitters.remove(i);
                self.status = format!("Effet « {} » retiré.", e.fx);
            }
            return;
        }
        t.emitters.push(Emitter { pos: p + v3(0.0, 0.0, 0.3), fx: f.name.to_string(), scale });
        self.status = format!("Effet « {} » posé ({} au total) · Ctrl + clic pour en retirer un.", f.label, t.emitters.len());
    }

    /// Effets posés : pastilles de couleur dans la vue (nom au survol de près), zone d'eau en cours de tracé.
    pub fn draw_scenery_overlay(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        for e in &t.emitters {
            let Some((q, _)) = self.cam.project(e.pos) else { continue };
            let cat = FX.iter().find(|f| f.name == e.fx).map(|f| f.cat).unwrap_or("");
            let col = fx_color(cat);
            painter.circle_filled(q, 6.0, col);
            painter.circle_stroke(q, 9.0, egui::Stroke::new(1.5_f32, col.gamma_multiply(0.6)));
            if hover.map(|h| h.distance(q) < 40.0).unwrap_or(false) || (e.pos - self.cam.eye).len() < 40.0 {
                let label = FX.iter().find(|f| f.name == e.fx).map(|f| f.label).unwrap_or(&e.fx);
                painter.text(q + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        if self.tool == crate::app::Tool::Terrain && self.terrain.brush == crate::terrain::Brush::Water {
            if let (Some(a), Some(h)) = (self.terrain.water_start, hover) {
                if let Some(b) = self.terrain_ground(h) {
                    let z = a.z + 0.2;
                    let c = [v3(a.x, a.y, z), v3(b.x, a.y, z), v3(b.x, b.y, z), v3(a.x, b.y, z)];
                    let pts: Vec<Pos2> = c.iter().filter_map(|p| self.cam.project(*p).map(|q| q.0)).collect();
                    if pts.len() == 4 {
                        painter.add(egui::Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(60, 140, 230, 90), egui::Stroke::new(2.0_f32, Color32::from_rgb(90, 170, 255))));
                    }
                }
            }
        }
    }

    pub fn terrain_ui_water(&mut self, ui: &mut egui::Ui) {
        ui.label("Glisser un rectangle dans la vue : l'eau monte à la hauteur du point de départ (+0,2 m).");
        ui.checkbox(&mut self.terrain.water_dig, "Creuser le bassin sous l'eau");
        if self.terrain.water_dig {
            ui.add(egui::Slider::new(&mut self.terrain.water_depth, 0.5..=10.0).text("profondeur").suffix(" m"));
        }
        let Some(t) = self.proj.terrain.as_ref() else { return };
        if !t.waters.is_empty() {
            ui.separator();
            ui.label(RichText::new("Zones d'eau").strong());
        }
        let mut del = None;
        let mut changed = false;
        let mut zs: Vec<f32> = t.waters.iter().map(|w| w.z).collect();
        for (i, w) in t.waters.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{:.0} × {:.0} m", w.max[0] - w.min[0], w.max[1] - w.min[1]));
                if ui.add(egui::DragValue::new(&mut zs[i]).speed(0.05).suffix(" m")).on_hover_text("Niveau de l'eau").changed() {
                    changed = true;
                }
                if ui.small_button("Supprimer").clicked() {
                    del = Some(i);
                }
            });
        }
        if changed {
            if let Some(t) = self.proj.terrain.as_mut() {
                for (w, z) in t.waters.iter_mut().zip(zs) {
                    w.z = z;
                }
            }
        }
        if let Some(i) = del {
            self.push_undo();
            if let Some(t) = self.proj.terrain.as_mut() {
                t.waters.remove(i);
            }
        }
        ui.label(RichText::new("En jeu : un water.xml (eau du jeu + tes zones) est chargé par un petit script client, l'eau est nageable.").small().weak());
    }

    pub fn terrain_ui_fx(&mut self, ui: &mut egui::Ui) {
        ui.label("Clic dans la vue : poser l'effet choisi · Ctrl + clic : retirer le plus proche.");
        let mut cat = "";
        for (i, f) in FX.iter().enumerate() {
            if f.cat != cat {
                cat = f.cat;
                ui.label(RichText::new(cat).strong().color(fx_color(cat)));
            }
            let on = self.terrain.fx == i;
            if ui.selectable_label(on, format!("   {}", f.label)).on_hover_text(f.name).clicked() {
                self.terrain.fx = i;
                self.set_terrain_brush(crate::terrain::Brush::Particles);
            }
        }
        ui.add(egui::Slider::new(&mut self.terrain.fx_scale, 0.2..=4.0).text("taille"));
        if let Some(t) = self.proj.terrain.as_ref() {
            ui.label(RichText::new(format!("{} effet(s) posé(s)", t.emitters.len())).small().weak());
        }
    }

    /// Export : water.xml complet + script client, et les effets (types d'objets dw_fx_* + modèle invisible).
    /// Renvoie (types d'objets XML, entités XML) à ajouter au ytyp / ymap du terrain.
    pub fn export_scenery(&self, stream: &std::path::Path, tiles: &mut u32) -> Result<(Vec<String>, String, Option<(V3, V3)>), String> {
        let Some(t) = self.proj.terrain.as_ref() else { return Ok((Vec::new(), String::new(), None)) };
        let root = stream.parent().unwrap_or(stream);
        let mut archs = Vec::new();
        let mut ents = String::new();
        let mut ext: Option<(V3, V3)> = None;
        // eau
        if !t.waters.is_empty() {
            let base = self.game.world.as_ref().and_then(|w| {
                w.vfs.files.iter().find(|f| f.name.eq_ignore_ascii_case("water.xml") && f.path.to_lowercase().contains("levels\\gta5")).and_then(|f| w.vfs.read_binary(f, &w.keys))
            });
            let mut s = base.map(|d| String::from_utf8_lossy(&d).to_string()).unwrap_or_else(|| "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<WaterData>\n <WaterQuads>\n </WaterQuads>\n <CalmingQuads />\n <WaveQuads />\n</WaterData>\n".to_string());
            let mut items = String::new();
            for w in &t.waters {
                items += &format!(
                    "  <Item>\n   <minX value=\"{:.1}\" />\n   <maxX value=\"{:.1}\" />\n   <minY value=\"{:.1}\" />\n   <maxY value=\"{:.1}\" />\n   <Type value=\"0\" />\n   <IsInvisible value=\"false\" />\n   <HasLimitedDepth value=\"false\" />\n   <z value=\"{:.2}\" />\n   <a1 value=\"26\" />\n   <a2 value=\"26\" />\n   <a3 value=\"26\" />\n   <a4 value=\"26\" />\n   <NoStencil value=\"false\" />\n  </Item>\n",
                    w.min[0], w.max[0], w.min[1], w.max[1], w.z
                );
            }
            match s.find("</WaterQuads>") {
                Some(p) => s.insert_str(p, &items),
                None => return Err("water.xml du jeu illisible".into()),
            }
            std::fs::write(root.join("dw_water.xml"), s).map_err(|e| e.to_string())?;
            std::fs::write(
                root.join("dw_water.lua"),
                "-- DragoWalker : charge l'eau du terrain (l'eau du jeu + les nouvelles zones)\nCreateThread(function()\n    LoadWaterFromPath(GetCurrentResourceName(), 'dw_water.xml')\nend)\n",
            )
            .map_err(|e| e.to_string())?;
        }
        // effets
        if !t.emitters.is_empty() {
            // modèle presque invisible : un minuscule triangle sous le point
            use crate::gta::drawable::{RGeom, RModel, RVert};
            let m = RModel {
                verts: vec![
                    RVert { pos: [0.0, 0.0, -0.05], nrm: [0.0, 0.0, 1.0], uv: [0.0, 0.0], col: [255; 4] },
                    RVert { pos: [0.01, 0.0, -0.05], nrm: [0.0, 0.0, 1.0], uv: [1.0, 0.0], col: [255; 4] },
                    RVert { pos: [0.0, 0.01, -0.05], nrm: [0.0, 0.0, 1.0], uv: [0.0, 1.0], col: [255; 4] },
                ],
                idx: vec![0, 1, 2],
                geoms: vec![RGeom { first: 0, count: 3, tex: 0, cutout: false, blend: 0, glass: false }],
                embedded_dict: None,
            };
            crate::sollumz::write_ydr_xml_ex(stream, "dw_fx_point", &m, &[0], &[], Some(crate::terrain::TERRAIN_TXD))?;
            let mut used: Vec<(String, f32)> = Vec::new();
            for e in &t.emitters {
                if !used.iter().any(|u| u.0 == e.fx && (u.1 - e.scale).abs() < 1e-3) {
                    used.push((e.fx.clone(), e.scale));
                }
            }
            for (fxn, sc) in &used {
                let name = format!("{}_{}", fx_arch(fxn), (sc * 100.0).round() as u32);
                archs.push(format!(
                    "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"150\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"-1\" y=\"-1\" z=\"-1\" />\n   <bbMax x=\"1\" y=\"1\" z=\"1\" />\n   <bsCentre x=\"0\" y=\"0\" z=\"0\" />\n   <bsRadius value=\"1.8\" />\n   <hdTextureDist value=\"50\" />\n   <name>{name}</name>\n   <textureDictionary />\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>dw_fx_point</assetName>\n   <extensions>\n    <Item type=\"CExtensionDefParticleEffect\">\n     <name>{name}</name>\n     <offsetPosition x=\"0\" y=\"0\" z=\"0\" />\n     <offsetRotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n     <fxName>{fxn}</fxName>\n     <fxType value=\"0\" />\n     <boneTag value=\"0\" />\n     <scale value=\"{sc}\" />\n     <probability value=\"100\" />\n     <flags value=\"0\" />\n     <color value=\"0xFFFFFFFF\" />\n    </Item>\n   </extensions>\n  </Item>"
                ));
            }
            for e in &t.emitters {
                *tiles += 1;
                let name = format!("{}_{}", fx_arch(&e.fx), (e.scale * 100.0).round() as u32);
                ents += &format!(
                    "  <Item type=\"CEntityDef\">\n   <archetypeName>{name}</archetypeName>\n   <flags value=\"{}\" />\n   <guid value=\"{}\" />\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"150\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>LODTYPES_DEPTH_HD</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
                    crate::terrain::TERRAIN_ENTITY_FLAGS,
                    0x0D77_0000u32 + *tiles,
                    e.pos.x,
                    e.pos.y,
                    e.pos.z
                );
                let (a, b) = (e.pos - v3(150.0, 150.0, 150.0), e.pos + v3(150.0, 150.0, 150.0));
                ext = Some(match ext {
                    Some((mn, mx)) => (mn.min(a), mx.max(b)),
                    None => (a, b),
                });
            }
        }
        Ok((archs, ents, ext))
    }
}
