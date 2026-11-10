//! Bibliothèque des bâtiments du jeu : tous les grands bâtiments posés sur la carte d'origine, rangés par
//! quartier, avec aperçu texturé. « Poser » copie le bâtiment (même modèle du jeu, même distance d'affichage)
//! à l'endroit voulu, par exemple sur une carte custom.
//!
//! À l'export, pour chaque bâtiment copié :
//! - dépendance de la carte vers le .ytyp du jeu qui le définit (_manifest.ymf, CPackFileMetaData >
//!   imapDependencies_2 > imapName / itypDepArray — format du _manifest.ymf.xml du vaisseau, validé en jeu) :
//!   les .ytyp de la carte du jeu ne sont chargés qu'avec leurs propres cartes ;
//! - collision générée à partir de son modèle (les bâtiments du jeu n'ont pas de collision propre : elle est
//!   dans les .ybn de leur quartier d'origine, qui ne suivent pas la copie).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, RichText};

pub struct Building {
    pub arch: u32,
    pub name: String,
    pub size: V3,
    /// un des endroits où le jeu le pose, et combien de fois
    pub pos: V3,
    pub count: usize,
    pub zone: usize,
}

pub struct BuildingLib {
    pub open: bool,
    pub filter: String,
    pub zone: usize,
    pub list: Option<Vec<Building>>,
    pub chosen: Option<u32>,
}

impl BuildingLib {
    pub fn new() -> BuildingLib {
        BuildingLib { open: std::env::var("DW_BUILDINGS").is_ok(), filter: std::env::var("DW_BUILDINGS_FILTER").unwrap_or_default(), zone: 0, list: None, chosen: None }
    }
}

/// Quartiers, d'après le début du nom des modèles de la carte (dt1_…, hw1_…) — **estimé**.
pub const ZONES: [(&str, &[&str]); 13] = [
    ("Tous", &[]),
    ("Centre-ville", &["dt1"]),
    ("Vinewood", &["hw1"]),
    ("Rockford Hills", &["bh1"]),
    ("Mirror Park", &["ss1"]),
    ("Del Perro / Vespucci", &["sm", "vb"]),
    ("Little Seoul", &["kt1"]),
    ("Sud de Los Santos", &["sc1"]),
    ("Zone industrielle", &["id1", "id2"]),
    ("Port", &["po1"]),
    ("Aéroport", &["ap1"]),
    ("Campagne", &["cs", "ch", "ce", "cn"]),
    ("Autres", &[]),
];

fn zone_of(name: &str) -> usize {
    let n = name.strip_prefix("hei_").unwrap_or(name);
    let tok = n.split('_').next().unwrap_or("");
    for (i, (_, pre)) in ZONES.iter().enumerate().skip(1) {
        if pre.iter().any(|p| tok.starts_with(p)) {
            return i;
        }
    }
    ZONES.len() - 1
}

/// Morceaux de carte qui ne sont pas des bâtiments (sol, routes, détails, versions lointaines…).
const NOT_BUILDING: [&str; 29] = ["_lod", "slod", "proxy", "decal", "detail", "_dtl", "emissive", "shadow", "ground", "land", "_rd", "rd_", "road", "water", "occl", "terrain", "_ovly", "overlay", "_glue", "railing", "hedge", "_weed", "cable", "logo", "grnd", "wire", "_alpha", "_repair", "_cut"];

/// Liste des bâtiments du jeu : objets HD (pas des props, pas des intérieurs) d'au moins 12 m de côté et 6 m
/// de haut — **estimé**.
/// Prop (et non morceau de carte) : même règle que « Props seulement » du catalogue.
pub fn is_prop_name(n: &str) -> bool {
    n.starts_with("prop_") || n.starts_with("p_") || n.starts_with("v_") || n.contains("_prop_")
}

pub fn build_list(w: &crate::gta::world::GameWorld) -> Vec<Building> {
    let mut map: std::collections::HashMap<u32, Building> = std::collections::HashMap::new();
    for e in &w.ents {
        if e.lod_level != 0 || e.mlo || w.hidden_archs.contains(&e.arch) {
            continue;
        }
        if let Some(b) = map.get_mut(&e.arch) {
            b.count += 1;
            continue;
        }
        let Some(a) = w.archetypes.get(&e.arch) else { continue };
        if a.mlo {
            continue;
        }
        let s = a.bb_max - a.bb_min;
        if s.x.max(s.y) < 12.0 || s.z < 6.0 {
            continue;
        }
        let name = w.name_of(e.arch);
        if is_prop_name(&name) || name.starts_with("hash_") || NOT_BUILDING.iter().any(|k| name.contains(k)) {
            continue;
        }
        let zone = zone_of(&name);
        map.insert(e.arch, Building { arch: e.arch, name, size: s, pos: e.pos, count: 1, zone });
    }
    let mut v: Vec<Building> = map.into_values().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

impl DwApp {
    /// Pose une copie du bâtiment (distance d'affichage du jeu, orientation 0) à un point du monde.
    pub fn building_place(&mut self, h: u32, at: V3) {
        let Some(w) = self.game.world.as_ref() else { return };
        let name = w.name_of(h);
        let lod = w.archetypes.get(&h).map(|a| a.lod_dist).unwrap_or(300.0).max(150.0);
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut e = crate::model::Entity::new(&name, at);
        e.rot = Quat::IDENTITY;
        e.lod_dist = lod;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        self.set_selection(vec![id]);
        self.snap_if_shift(&[id]);
        self.sync_game_project();
        self.status = format!("Bâtiment « {name} » copié à {:.0} {:.0} {:.0} (visible jusqu'à {lod:.0} m). À l'export : dépendance vers son .ytyp et collision générée.", at.x, at.y, at.z);
    }

    /// Fenêtre « Bibliothèque des bâtiments ».
    pub fn buildings_window(&mut self, ctx: &egui::Context) {
        if !self.buildings.open {
            return;
        }
        let mut open = true;
        let mut place: Option<(u32, Option<V3>)> = None;
        let mut goto: Option<(V3, f32)> = None;
        self.show_panel(ctx, "batiments", "Bibliothèque des bâtiments", crate::docking::Dock::Floating, [860.0, 600.0], &mut open, |s, ui| {
                let Some(w) = s.game.world.as_ref() else {
                    ui.label("Charge d'abord le jeu (onglet Jeu).");
                    return;
                };
                if s.buildings.list.is_none() {
                    s.buildings.list = Some(build_list(w));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(&mut s.buildings.filter).hint_text("nom : dt1_05, build, tower…").desired_width(220.0));
                    for (i, (n, _)) in ZONES.iter().enumerate() {
                        let on = s.buildings.zone == i;
                        let b = egui::Button::new(if on { RichText::new(*n).strong().color(Color32::BLACK) } else { RichText::new(*n) }).fill(if on { Color32::from_rgb(120, 190, 255) } else { Color32::from_gray(45) }).corner_radius(10.0);
                        if ui.add(b).clicked() {
                            s.buildings.zone = i;
                        }
                    }
                });
                let f = s.buildings.filter.to_lowercase();
                let zone = s.buildings.zone;
                // la liste est sortie le temps de dessiner (l'aperçu a besoin de toute l'application)
                let list = s.buildings.list.take().unwrap_or_default();
                let idx: Vec<usize> = (0..list.len()).filter(|&i| (zone == 0 || list[i].zone == zone) && (f.is_empty() || list[i].name.contains(&f))).collect();
                ui.label(RichText::new(format!("{} bâtiments · clic : voir · double-clic : poser devant la caméra", idx.len())).small().weak());
                ui.separator();
                let mut chosen = s.buildings.chosen;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(360.0);
                        egui::ScrollArea::vertical().id_salt("bld_list").auto_shrink([false; 2]).max_height(ui.available_height().max(300.0)).show_rows(ui, 22.0, idx.len(), |ui, range| {
                            for k in range {
                                let b = &list[idx[k]];
                                let r = ui.selectable_label(chosen == Some(b.arch), format!("{}  ·  {:.0} × {:.0} × {:.0} m", b.name, b.size.x, b.size.y, b.size.z));
                                if r.clicked() {
                                    chosen = Some(b.arch);
                                }
                                if r.double_clicked() {
                                    place = Some((b.arch, None));
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let Some(h) = chosen else {
                            ui.label("Choisis un bâtiment dans la liste.");
                            return;
                        };
                        let Some(b) = list.iter().find(|b| b.arch == h) else { return };
                        let (name, size, pos, count, zone_n) = (b.name.clone(), b.size, b.pos, b.count, ZONES[b.zone].0);
                        ui.label(RichText::new(&name).strong());
                        ui.label(format!("{:.0} × {:.0} × {:.0} m · {zone_n} · posé {count} fois dans le jeu", size.x, size.y, size.z));
                        s.textured_preview(ui, h, &name);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(RichText::new("Poser devant la caméra").strong()).clicked() {
                                place = Some((h, None));
                            }
                            if ui.button("Poser au centre de la vue").clicked() {
                                place = Some((h, Some(s.cam.target)));
                            }
                            if ui.button("Aller voir l'original").clicked() {
                                goto = Some((pos, size.x.max(size.y).max(size.z) * 1.6 + 20.0));
                            }
                        });
                        ui.label(RichText::new("La copie utilise le modèle et les textures du jeu ; à l'export, DragoWalker déclare son .ytyp d'origine et génère sa collision.").small().weak());
                    });
                });
                s.buildings.chosen = chosen;
                s.buildings.list = Some(list);
            });
        self.buildings.open = open;
        if let Some((h, at)) = place {
            let name = self.game.world.as_ref().map(|w| w.name_of(h)).unwrap_or_default();
            let p = at.unwrap_or_else(|| {
                // devant la caméra, posé au sol (GetSpawnPos 0x06000dfb : 2,5 × le rayon)
                let s = self.spawn_pos(&name);
                let z = self.mouse_ground.map(|g| g.z).or_else(|| self.proj.terrain.as_ref().and_then(|t| t.height_at(s.x, s.y))).unwrap_or(s.z);
                v3(s.x, s.y, z)
            });
            self.building_place(h, p);
        }
        if let Some((p, d)) = goto {
            self.game.empty_map = false;
            self.goto(p, d);
        }
    }

    /// Export : manifeste des dépendances (_manifest.ymf) du projet et collisions des bâtiments du jeu copiés.
    pub fn export_buildings(&self, stream: &std::path::Path) -> Result<Option<String>, String> {
        // fichiers d'un export précédent
        if let Ok(rd) = std::fs::read_dir(stream) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                if (n.starts_with("dw_bcol_") || n.starts_with("_manifest.ymf")) && f.path().is_file() {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
        let w = self.game.world.as_deref();
        let names = crate::terrain::material_names(w);
        let concrete = names.iter().position(|n| n == "CONCRETE").unwrap_or(1) as u32;
        // .ytyp qui définit un type d'objet : d'abord les .ytyp du projet, puis ceux du jeu
        let ytyp_of = |arch: &str| -> Option<String> {
            let low = arch.to_lowercase();
            for t in &self.proj.ytyps {
                if t.mlos.iter().any(|m| m.name.eq_ignore_ascii_case(&low)) || t.raw_archetypes.iter().any(|r| r.to_lowercase().contains(&format!("<name>{low}</name>"))) {
                    return Some(t.name.clone());
                }
            }
            let w = w?;
            let a = w.archetypes.get(&crate::gta::crypto::joaat(&low))?;
            Some(w.vfs.files[a.ytyp_file].name.trim_end_matches(".ytyp").to_string())
        };
        // comme EditProjectManifestPanel::GenerateProjectManifest (0x06001194) :
        // chaque carte déclare les .ytyp de tous ses objets ; INTERIOR_DATA si elle place un intérieur ;
        // chaque .ytyp d'intérieur déclare les .ytyp des objets de l'intérieur (sauf lui-même) ;
        // les intérieurs sont listés dans <Interiors> (Name, Bounds = leur nom)
        let mut imaps: Vec<(String, bool, Vec<String>)> = Vec::new();
        let mut ncol = 0;
        for y in self.proj.ymaps.iter().filter(|y| !y.sample) {
            let mut itypes: Vec<String> = Vec::new();
            let mut interior = false;
            for e in &y.entities {
                if e.mlo.is_some() {
                    interior = true;
                }
                if let Some(yt) = ytyp_of(&e.archetype) {
                    if !itypes.contains(&yt) {
                        itypes.push(yt);
                    }
                }
                // bâtiment du jeu copié : collision depuis son modèle, dans le monde
                let Some(w) = w else { continue };
                let h = crate::gta::crypto::joaat(&e.archetype.to_lowercase());
                let Some(a) = w.archetypes.get(&h) else { continue };
                if is_prop_name(&e.archetype.to_lowercase()) || a.mlo || e.archetype.starts_with("dw_") {
                    continue;
                }
                if let Some(m) = w.load_model(h, 60000) {
                    let s = v3(e.scale_xy, e.scale_xy, e.scale_z);
                    let verts: Vec<V3> = m.pos.iter().map(|p| e.pos + e.rot.rotate(v3(p[0], p[1], p[2]).mulv(s))).collect();
                    let tris: Vec<(u32, u32, u32, u8)> = m.tris.iter().map(|t| (t[0], t[1], t[2], 0u8)).collect();
                    if !tris.is_empty() {
                        std::fs::write(stream.join(format!("dw_bcol_{ncol}.ybn.xml")), crate::roads::mesh_collision_xml(&verts, &tris, &[concrete])).map_err(|e| e.to_string())?;
                        ncol += 1;
                    }
                }
            }
            if !itypes.is_empty() {
                imaps.push((y.name.clone(), interior, itypes));
            }
        }
        if self.proj.terrain.is_some() {
            imaps.push(("dw_terrain_placement".into(), false, vec!["dw_terrain_types".into()]));
        }
        let mut ityps: Vec<(String, Vec<String>)> = Vec::new();
        let mut interiors: Vec<String> = Vec::new();
        for t in &self.proj.ytyps {
            let mut list: Vec<String> = Vec::new();
            for m in &t.mlos {
                interiors.push(m.name.clone());
                for e in m.entities.iter().chain(m.entity_sets.iter().flat_map(|s| s.entities.iter())) {
                    if let Some(yt) = ytyp_of(&e.archetype) {
                        if yt != t.name && !list.contains(&yt) {
                            list.push(yt);
                        }
                    }
                }
            }
            if !list.is_empty() {
                ityps.push((t.name.clone(), list));
            }
        }
        if imaps.is_empty() && ityps.is_empty() {
            return Ok(None);
        }
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n<CPackFileMetaData>\n  <MapDataGroups/>\n  <HDTxdBindingArray/>\n  <imapDependencies/>\n");
        if imaps.is_empty() {
            s += "  <imapDependencies_2/>\n";
        } else {
            s += "  <imapDependencies_2>\n";
            for (ymap, interior, list) in &imaps {
                s += &format!("    <Item>\n      <imapName>{ymap}</imapName>\n");
                s += if *interior { "      <manifestFlags>INTERIOR_DATA</manifestFlags>\n" } else { "      <manifestFlags/>\n" };
                s += "      <itypDepArray>\n";
                for t in list {
                    s += &format!("        <Item>{t}</Item>\n");
                }
                s += "      </itypDepArray>\n    </Item>\n";
            }
            s += "  </imapDependencies_2>\n";
        }
        if ityps.is_empty() {
            s += "  <itypDependencies_2/>\n";
        } else {
            s += "  <itypDependencies_2>\n";
            for (ytyp, list) in &ityps {
                s += &format!("    <Item>\n      <itypName>{ytyp}</itypName>\n      <manifestFlags>INTERIOR_DATA</manifestFlags>\n      <itypDepArray>\n");
                for t in list {
                    s += &format!("        <Item>{t}</Item>\n");
                }
                s += "      </itypDepArray>\n    </Item>\n";
            }
            s += "  </itypDependencies_2>\n";
        }
        if interiors.is_empty() {
            s += "  <Interiors/>\n";
        } else {
            s += "  <Interiors itemType=\"CInteriorBoundsFiles\">\n";
            for n in &interiors {
                s += &format!("    <Item>\n      <Name>{n}</Name>\n      <Bounds>\n        <Item>{n}</Item>\n      </Bounds>\n    </Item>\n");
            }
            s += "  </Interiors>\n";
        }
        s += "</CPackFileMetaData>\n";
        std::fs::write(stream.join("_manifest.ymf.xml"), s).map_err(|e| e.to_string())?;
        let n: usize = imaps.iter().map(|d| d.2.len()).sum::<usize>() + ityps.iter().map(|d| d.1.len()).sum::<usize>();
        Ok(Some(format!("Manifeste _manifest.ymf : {n} dépendance(s) .ytyp ({} carte(s), {} intérieur(s)) ; {ncol} collision(s) de bâtiment générée(s).", imaps.len(), interiors.len())))
    }
}
