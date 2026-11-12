//! Listes du jeu (scénarios de piétons, LOD, lumières) et sélection intelligente. Les intérieurs sont dans
//! leur bibliothèque à part (intlib.rs).
//! Structures lues avec les dispositions de CodeWalker (MetaTypes::GetStructureInfo) :
//! CScenarioPoint 64 o (vPositionAndDirection à +48), CScenarioChainingNode 32 o (Position à +0),
//! CScenarioChainingEdge 8 o (NodeIndexFrom +0, NodeIndexTo +2), CMapData.DistantLODLightsSOA à +392
//! (CDistantLODLight : position +8, RGBI +24, numStreetLights +40).

use crate::app::{DwApp, Tab};
use crate::gta::meta::Meta;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, RichText, Stroke};

/// Codes de structure (MetaName de CodeWalker).
const H_SCENARIO_POINT: u32 = 4103049490;
const H_CHAIN_NODE: u32 = 3340683255;
const H_CHAIN_EDGE: u32 = 4255409560;

pub struct ScenFile {
    pub name: String,
    pub points: Vec<V3>,
    pub nodes: Vec<V3>,
    pub edges: Vec<(u16, u16)>,
    pub center: V3,
}

pub struct LightFile {
    pub name: String,
    pub lights: Vec<(V3, [u8; 3])>,
    pub center: V3,
}

#[derive(Default)]
pub struct Lists {
    pub sub: u8,
    pub filter: String,
    pub scen: Option<Vec<ScenFile>>,
    pub lights: Option<Vec<LightFile>>,
    pub show_scen: bool,
    pub show_lights: bool,
}

fn block_len(m: &Meta, pos: usize) -> usize {
    m.blocks.iter().find(|b| pos >= b.1 && pos < b.1 + b.2.max(1)).map(|b| b.1 + b.2 - pos).unwrap_or(0)
}

/// Points et trajets d'un fichier de scénarios (.ymt).
fn parse_scenarios(r: &crate::gta::rpf::Resource, name: &str) -> Option<ScenFile> {
    let m = Meta::parse(r)?;
    let mut f = ScenFile { name: name.to_string(), points: Vec::new(), nodes: Vec::new(), edges: Vec::new(), center: V3::ZERO };
    for &(h, pos, len) in &m.blocks {
        match h {
            H_SCENARIO_POINT => {
                for i in 0..len / 64 {
                    let p = pos + i * 64 + 48;
                    f.points.push(v3(r.f32(p), r.f32(p + 4), r.f32(p + 8)));
                }
            }
            H_CHAIN_NODE => {
                for i in 0..len / 32 {
                    let p = pos + i * 32;
                    f.nodes.push(v3(r.f32(p), r.f32(p + 4), r.f32(p + 8)));
                }
            }
            H_CHAIN_EDGE => {
                for i in 0..len / 8 {
                    let p = pos + i * 8;
                    f.edges.push((r.u16(p), r.u16(p + 2)));
                }
            }
            _ => {}
        }
    }
    let all: Vec<&V3> = f.points.iter().chain(f.nodes.iter()).collect();
    if all.is_empty() {
        return None;
    }
    f.center = all.iter().fold(V3::ZERO, |a, p| a + **p) * (1.0 / all.len() as f32);
    Some(f)
}

/// Lumières lointaines (CDistantLODLight) d'un ymap « lodlights ».
fn parse_lights(r: &crate::gta::rpf::Resource, name: &str) -> Option<LightFile> {
    let m = Meta::parse(r)?;
    let root = m.first_block(crate::gta::meta::H_CMAPDATA)?;
    let soa = root + 392;
    let (_, ppos, n) = m.array(soa + 8)?;
    // positions : MetaVECTOR3, 12 octets (YmapDistantLODLights.positions, YmapLODLight::Init 0x06002da9)
    let stride = 12;
    let _ = block_len(&m, ppos);
    let cols = m.array(soa + 24);
    let mut f = LightFile { name: name.to_string(), lights: Vec::with_capacity(n), center: V3::ZERO };
    for i in 0..n {
        let p = ppos + i * stride;
        let c = cols.map(|(_, cp, cn)| if i < cn { r.u32(cp + i * 4) } else { 0xFFFF_FFFF }).unwrap_or(0xFFFF_FFFF);
        // couleur : Color.FromBgra (YmapLODLight::Init 0x06002da9) — bleu en bas, puis vert, rouge, intensité en haut
        f.lights.push((v3(r.f32(p), r.f32(p + 4), r.f32(p + 8)), [(c >> 16) as u8, (c >> 8) as u8, c as u8]));
    }
    if f.lights.is_empty() {
        return None;
    }
    f.center = f.lights.iter().fold(V3::ZERO, |a, l| a + l.0) * (1.0 / f.lights.len() as f32);
    Some(f)
}

const LOD_NAMES: [&str; 6] = ["HD", "LOD", "SLOD1", "SLOD2", "SLOD3", "HD seul"];

/// Distance d'apparition arrondie à 10 m (groupes de la liste des LOD).
pub fn lod_bucket(d: f32) -> u32 {
    ((d / 10.0).round() * 10.0).max(0.0) as u32
}

impl DwApp {
    fn load_scenarios(&mut self) {
        let Some(w) = self.game.world.as_ref() else { return };
        let mut out = Vec::new();
        for f in w.vfs.files.iter().filter(|f| f.name.ends_with(".ymt") && f.path().to_lowercase().contains("scenario")) {
            if let Some(r) = w.vfs.read_resource(f, &w.keys) {
                if let Some(s) = parse_scenarios(&r, f.name.trim_end_matches(".ymt")) {
                    out.push(s);
                }
            }
        }
        // le même fichier existe dans le jeu de base et dans les mises à jour : on garde la dernière version
        let mut seen = std::collections::HashMap::new();
        for (i, s) in out.iter().enumerate() {
            seen.insert(s.name.clone(), i);
        }
        let keep: std::collections::HashSet<usize> = seen.values().copied().collect();
        let out: Vec<ScenFile> = out.into_iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, s)| s).collect();
        let (p, n): (usize, usize) = out.iter().fold((0, 0), |a, s| (a.0 + s.points.len(), a.1 + s.edges.len()));
        self.status = format!("{} fichiers de scénarios : {p} points et {n} morceaux de trajets.", out.len());
        self.lists.scen = Some(out);
    }

    fn load_lights(&mut self) {
        let Some(w) = self.game.world.as_ref() else { return };
        let mut out = Vec::new();
        // ces cartes n'ont pas d'objets : on les lit directement dans les archives du jeu
        for f in w.vfs.files.iter().filter(|f| f.name.ends_with(".ymap") && f.name.to_lowercase().contains("lodlights")) {
            if let Some(r) = w.vfs.read_resource(f, &w.keys) {
                if let Some(l) = parse_lights(&r, f.name.trim_end_matches(".ymap")) {
                    out.push(l);
                }
            }
        }
        let n: usize = out.iter().map(|l| l.lights.len()).sum();
        self.status = format!("{} fichiers de lumières lointaines : {n} lumières.", out.len());
        self.lists.lights = Some(out);
    }

    /// Test : charge scénarios et lumières, les affiche dans la vue.
    pub fn lists_test(&mut self) {
        self.load_scenarios();
        println!("{}", self.status);
        self.load_lights();
        println!("{}", self.status);
        if let Some(sc) = &self.lists.scen {
            if let Some(s) = sc.iter().min_by(|a, b| a.center.dist2d(self.cam.target).partial_cmp(&b.center.dist2d(self.cam.target)).unwrap_or(std::cmp::Ordering::Equal)) {
                println!("   scénario le plus proche : {} ({} points, {} trajets)", s.name, s.points.len(), s.edges.len());
            }
        }
        self.lists.show_scen = true;
        self.lists.show_lights = true;
        self.tab = Tab::Lists;
    }

    /// Onglet « Listes ».
    pub fn lists_tab(&mut self, ui: &mut egui::Ui) {
        if self.game.world.is_none() {
            ui.label("Charge d'abord le jeu (onglet Jeu).");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            // les intérieurs ont leur propre bibliothèque (bouton « Intérieurs ») : plus de sous-liste ici
            for (i, l) in [(0u8, "Scénarios (piétons)"), (2, "LOD"), (3, "Lumières")] {
                if ui.selectable_label(self.lists.sub == i, l).clicked() {
                    self.lists.sub = i;
                }
            }
        });
        if self.lists.sub == 1 {
            self.lists.sub = 0;
        }
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.lists.filter).hint_text("filtrer par nom").desired_width(200.0));
        });
        ui.separator();
        let f = self.lists.filter.to_lowercase();
        let t = self.cam.target;
        let mut goto: Option<(V3, f32)> = None;
        let mut pick: Option<u32> = None;
        match self.lists.sub {
            0 => {
                if self.lists.scen.is_none() {
                    if ui.button("Charger les scénarios du jeu").clicked() {
                        self.load_scenarios();
                    }
                    ui.label(RichText::new("Points où les piétons font une activité et trajets qu'ils suivent (fichiers .ymt du dossier scenario).").weak().small());
                } else {
                    ui.checkbox(&mut self.lists.show_scen, "Afficher dans la vue (points bleus, trajets orange)");
                    let mut v: Vec<(f32, usize)> = self.lists.scen.as_ref().unwrap().iter().enumerate().filter(|(_, s)| f.is_empty() || s.name.to_lowercase().contains(&f)).map(|(i, s)| (s.center.dist2d(t), i)).collect();
                    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    ui.label(RichText::new(format!("{} fichiers, du plus proche au plus loin", v.len())).small().weak());
                    egui::ScrollArea::vertical().id_salt("scen_list").max_height(420.0).show_rows(ui, 20.0, v.len(), |ui, range| {
                        for k in range {
                            let s = &self.lists.scen.as_ref().unwrap()[v[k].1];
                            ui.horizontal(|ui| {
                                ui.label(format!("{} · {} points · {} trajets · {:.0} m", s.name, s.points.len(), s.edges.len(), v[k].0));
                                if ui.small_button("Aller voir").clicked() {
                                    goto = Some((s.center, 150.0));
                                }
                            });
                        }
                    });
                }
            }
            2 => {
                let w = self.game.world.as_ref().unwrap();
                let mut counts = [0usize; 6];
                let mut v: Vec<(f32, u32)> = Vec::new();
                for (i, e) in w.ents.iter().enumerate() {
                    if !w.ymaps[e.ymap as usize].visible || e.pos.dist2d(t) > 600.0 {
                        continue;
                    }
                    counts[(e.lod_level as usize).min(5)] += 1;
                    if (1..=4).contains(&e.lod_level) && (f.is_empty() || w.name_of(e.arch).to_lowercase().contains(&f)) {
                        v.push((e.pos.dist2d(t), i as u32));
                    }
                }
                v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                ui.label(format!("À moins de 600 m : {}", (0..6).map(|i| format!("{} {}", counts[i], LOD_NAMES[i])).collect::<Vec<_>>().join(" · ")));
                // groupes par niveau et distance d'apparition (lodDist arrondi à 10 m), à afficher ou cacher
                let mut groups: std::collections::BTreeMap<(u8, u32), usize> = std::collections::BTreeMap::new();
                for e in w.ents.iter() {
                    if w.ymaps[e.ymap as usize].visible && e.pos.dist2d(t) <= 600.0 {
                        *groups.entry((e.lod_level.min(5), lod_bucket(e.lod_dist()))).or_insert(0) += 1;
                    }
                }
                ui.label(RichText::new("Par distance d'apparition (décoche pour cacher dans la vue) :").strong());
                let mut only: Option<(u8, u32)> = None;
                let mut toggle: Option<((u8, u32), bool)> = None;
                egui::ScrollArea::vertical().id_salt("lod_groups").max_height(200.0).show(ui, |ui| {
                    for (k, n) in &groups {
                        let mut on = !self.game.hidden_lod.contains(k);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut on, format!("{} · apparaît à {} m · {n} objet(s)", LOD_NAMES[k.0 as usize], k.1)).changed() {
                                toggle = Some((*k, on));
                            }
                            if ui.small_button("Voir seulement celui-ci").clicked() {
                                only = Some(*k);
                            }
                        });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Tout afficher").clicked() {
                        self.game.hidden_lod.clear();
                    }
                    ui.label(RichText::new(format!("{} groupe(s) caché(s)", self.game.hidden_lod.len())).small().weak());
                });
                if let Some((k, on)) = toggle {
                    if on {
                        self.game.hidden_lod.remove(&k);
                    } else {
                        self.game.hidden_lod.insert(k);
                    }
                }
                if let Some(k) = only {
                    self.game.hidden_lod = groups.keys().filter(|g| **g != k).copied().collect();
                    self.status = format!("Seulement les {} qui apparaissent à {} m.", LOD_NAMES[k.0 as usize], k.1);
                }
                ui.separator();
                ui.label(RichText::new("Versions lointaines (LOD, SLOD) des bâtiments et du décor :").small().weak());
                egui::ScrollArea::vertical().id_salt("lod_list").max_height(420.0).show_rows(ui, 20.0, v.len(), |ui, range| {
                    for k in range {
                        let e = &w.ents[v[k].1 as usize];
                        ui.horizontal(|ui| {
                            ui.label(format!("{} · {} · {:.0} m", w.name_of(e.arch), LOD_NAMES[(e.lod_level as usize).min(5)], v[k].0));
                            if ui.small_button("Aller voir").clicked() {
                                goto = Some((e.pos, 120.0));
                            }
                            if ui.small_button("Choisir").clicked() {
                                pick = Some(v[k].1);
                            }
                        });
                    }
                });
            }
            _ => {
                if self.lists.lights.is_none() {
                    if ui.button("Charger les lumières du jeu").clicked() {
                        self.load_lights();
                    }
                    ui.label(RichText::new("Lumières lointaines des rues et bâtiments (fichiers lodlights des cartes).").weak().small());
                } else {
                    ui.checkbox(&mut self.lists.show_lights, "Afficher dans la vue (points de couleur)");
                    let mut v: Vec<(f32, usize)> = self.lists.lights.as_ref().unwrap().iter().enumerate().filter(|(_, l)| f.is_empty() || l.name.to_lowercase().contains(&f)).map(|(i, l)| (l.center.dist2d(t), i)).collect();
                    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    egui::ScrollArea::vertical().id_salt("light_list").max_height(420.0).show_rows(ui, 20.0, v.len(), |ui, range| {
                        for k in range {
                            let l = &self.lists.lights.as_ref().unwrap()[v[k].1];
                            ui.horizontal(|ui| {
                                ui.label(format!("{} · {} lumières · {:.0} m", l.name, l.lights.len(), v[k].0));
                                if ui.small_button("Aller voir").clicked() {
                                    goto = Some((l.center, 250.0));
                                }
                            });
                        }
                    });
                }
            }
        }
        if let Some((p, d)) = goto {
            self.goto(p, d);
        }
        if let Some(ei) = pick {
            self.push_undo();
            let ids = self.game_ents_to_project(&[ei]);
            self.set_selection(ids);
        }
    }

    /// Points de scénario, trajets et lumières par-dessus la vue (autour du centre de la vue).
    pub fn draw_lists_overlay(&self, painter: &egui::Painter) {
        let t = self.cam.target;
        if self.lists.show_scen {
            if let Some(sc) = &self.lists.scen {
                for s in sc.iter().filter(|s| s.center.dist2d(t) < 1500.0) {
                    for (a, b) in &s.edges {
                        if let (Some(pa), Some(pb)) = (s.nodes.get(*a as usize), s.nodes.get(*b as usize)) {
                            if pa.dist2d(t) > 700.0 {
                                continue;
                            }
                            if let (Some((qa, _)), Some((qb, _))) = (self.cam.project(*pa), self.cam.project(*pb)) {
                                painter.line_segment([qa, qb], Stroke::new(2.0_f32, Color32::from_rgb(255, 150, 40)));
                            }
                        }
                    }
                    for p in s.points.iter().filter(|p| p.dist2d(t) < 700.0) {
                        if let Some((q, _)) = self.cam.project(*p) {
                            painter.circle_filled(q, 3.0, Color32::from_rgb(70, 170, 255));
                        }
                    }
                }
            }
        }
        if self.lists.show_lights {
            if let Some(ls) = &self.lists.lights {
                for l in ls.iter().filter(|l| l.center.dist2d(t) < 2500.0) {
                    for (p, c) in l.lights.iter().filter(|(p, _)| p.dist2d(t) < 1200.0) {
                        if let Some((q, _)) = self.cam.project(*p) {
                            painter.circle_filled(q, 2.5, Color32::from_rgb(c[0].max(60), c[1].max(60), c[2].max(60)));
                        }
                    }
                }
            }
        }
    }

    /// Sélection intelligente : à partir de l'objet choisi, prend tout ce qui est dedans ou dessus (props,
    /// décalques, intérieurs, versions lointaines…), par exemple tout un yacht d'un coup.
    pub fn smart_select(&mut self) {
        let Some(id) = self.selected.first().copied() else {
            self.status = "Choisis d'abord un objet (le yacht, le bâtiment…).".into();
            return;
        };
        let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)) else { return };
        let (mn, mx) = e.local_box(&self.proj);
        let (sxy, sz) = (e.scale_xy.max(0.01), e.scale_z.max(0.01));
        let inv = e.rot.inverse();
        let margin = 1.5f32;
        let inside = |p: V3| {
            let l = inv.rotate(p - e.pos);
            let l = v3(l.x / sxy, l.y / sxy, l.z / sz);
            l.x >= mn.x - margin && l.x <= mx.x + margin && l.y >= mn.y - margin && l.y <= mx.y + margin && l.z >= mn.z - margin && l.z <= mx.z + margin
        };
        // objets du projet
        let mut ids: Vec<u64> = self.proj.all_items().into_iter().filter_map(|r| self.proj.world_entity(r)).filter(|x| inside(x.pos)).map(|x| x.id).collect();
        // objets du jeu (toutes versions, carte pas encore copiée)
        let mut game: Vec<u32> = Vec::new();
        let (mut lods, mut decals, mut ints) = (0, 0, 0);
        if let Some(w) = self.game.world.as_ref() {
            for (i, g) in w.ents.iter().enumerate() {
                let y = &w.ymaps[g.ymap as usize];
                if !y.visible || y.in_project || !inside(g.pos) {
                    continue;
                }
                let n = w.name_of(g.arch);
                if (1..=4).contains(&g.lod_level) {
                    lods += 1;
                }
                if n.contains("decal") || n.contains("dtl") || n.contains("ovly") {
                    decals += 1;
                }
                if g.mlo {
                    ints += 1;
                }
                game.push(i as u32);
            }
        }
        if !game.is_empty() {
            self.push_undo();
            ids.extend(self.game_ents_to_project(&game));
        }
        ids.sort();
        ids.dedup();
        let n = ids.len();
        self.set_selection(ids);
        self.tab = Tab::Selection;
        self.status = format!("Sélection intelligente autour de « {} » : {n} objet(s), dont {decals} décalque(s), {ints} intérieur(s), {lods} version(s) lointaine(s).", e.archetype);
    }
}
