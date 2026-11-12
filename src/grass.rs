//! Herbe du jeu et biomes du créateur de terrain.
//!
//! **Retirés de l'interface le 11 nov. 2026** (demande de la personne : pas encore vus fonctionner en jeu) ; le
//! code reste prêt (export des étiquettes dans la collision, semis des biomes) pour être repris — voir A_FAIRE.md.
//!
//! - Herbe : dans GTA V, l'herbe, les fleurs et les petites plantes du sol ne sont pas des objets posés : le jeu les
//!   fait pousser lui-même sur les surfaces de collision dont le matériau porte une étiquette de végétation
//!   procédurale (ProceduralID, bits 8–15 du matériau : CodeWalker BoundMaterial_s::get_ProceduralId 0x060042e0).
//!   Le numéro d'une étiquette est sa place dans la table procTagTable de procedural.meta (fichier du jeu). Le
//!   pinceau « Herbe du jeu » range une étiquette par case du terrain ; l'export la met dans la collision.
//! - Biomes : un mélange d'ensembles de végétation (vegetation::SETS) et une herbe du jeu, semés sur tout le
//!   terrain d'un coup.

use crate::app::DwApp;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, RichText};

pub struct GrassKind {
    pub name: &'static str,
    pub tip: &'static str,
    /// nom dans procTagTable (procedural.meta)
    pub tag: &'static str,
    /// couleur des touffes dans l'éditeur
    pub color: [u8; 3],
}

/// Herbes du jeu proposées : étiquettes de procedural.meta (relevées dans update.rpf, 11 nov. 2026).
pub const KINDS: [GrassKind; 11] = [
    GrassKind { name: "Prairie verte", tip: "Herbe verte de prairie (Green_Meadow_01)", tag: "Green_Meadow_01", color: [90, 200, 80] },
    GrassKind { name: "Prairie fleurie", tip: "Herbe et fleurs des champs (Green_Meadow_Flowers)", tag: "Green_Meadow_Flowers", color: [210, 220, 90] },
    GrassKind { name: "Herbe touffue", tip: "Herbe haute et buissonnante (Green_Meadow_Bushy_01)", tag: "Green_Meadow_Bushy_01", color: [60, 160, 60] },
    GrassKind { name: "Herbe jaune", tip: "Herbe jaunie de prairie (meadow_yellow_grass)", tag: "meadow_yellow_grass", color: [200, 180, 70] },
    GrassKind { name: "Herbes sèches", tip: "Mauvaises herbes sèches, désert (grass_dry_weeds_browner)", tag: "grass_dry_weeds_browner", color: [170, 130, 70] },
    GrassKind { name: "Colline luxuriante", tip: "Végétation dense de colline (HILLSIDE_LUSH)", tag: "HILLSIDE_LUSH", color: [50, 150, 70] },
    GrassKind { name: "Montagne verte", tip: "Flanc de montagne verdoyant (MOUNTAINSIDE_LUSH)", tag: "MOUNTAINSIDE_LUSH", color: [70, 140, 90] },
    GrassKind { name: "Montagne sèche", tip: "Flanc de montagne sec (MOUNTAINSIDE_DRY)", tag: "MOUNTAINSIDE_DRY", color: [160, 140, 90] },
    GrassKind { name: "Côte", tip: "Plantes de bord de mer (Coastal_Meadow_Plants)", tag: "Coastal_Meadow_Plants", color: [120, 190, 140] },
    GrassKind { name: "Bord de rivière", tip: "Roseaux et plantes de berge (RIVER_BANK)", tag: "RIVER_BANK", color: [80, 170, 160] },
    GrassKind { name: "Fleurs des hauteurs", tip: "Fleurs sauvages (PROC_HIGH_FLOWERS)", tag: "PROC_HIGH_FLOWERS", color: [230, 120, 200] },
];

/// Une collision de terrain a un matériau par couple (surface, herbe) : 7 × (herbes + 1). CodeWalker ne garde que
/// les matériaux vraiment utilisés par les triangles (BoundGeometry::BuildMaterials 0x06004123, appelé par
/// ReadXml), mais leur nombre et l'index de chaque triangle tiennent sur un octet (set_MaterialsCount 0x060040ec,
/// PolygonMaterialIndices byte[]) : 256 matériaux au plus par morceau de collision.
const _: () = assert!((KINDS.len() + 1) * 7 <= 256);

/// Noms de la table procTagTable de procedural.meta, dans l'ordre (la place = le ProceduralID). Le fichier de
/// update.rpf passe avant celui de common.rpf.
pub fn proc_tags(w: Option<&crate::gta::world::GameWorld>) -> Vec<String> {
    let Some(w) = w else { return Vec::new() };
    let mut files: Vec<&crate::gta::rpf::FileEntry> = w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case("procedural.meta")).collect();
    files.sort_by_key(|f| !f.path.contains("update"));
    let Some(d) = files.first().and_then(|f| w.vfs.read_binary(f, &w.keys)) else { return Vec::new() };
    let s = String::from_utf8_lossy(&d);
    let Some(a) = s.find("<procTagTable>") else { return Vec::new() };
    let end = s[a..].find("</procTagTable>").map(|e| a + e).unwrap_or(s.len());
    s[a..end].split("<name>").skip(1).filter_map(|p| p.find("</name>").map(|e| p[..e].trim().to_string())).collect()
}

/// ProceduralID de chaque herbe : [0 (sans herbe), herbe 1, herbe 2…] ; 0 si l'étiquette n'est pas trouvée.
pub fn proc_ids(w: Option<&crate::gta::world::GameWorld>) -> Vec<u32> {
    let tags = proc_tags(w);
    std::iter::once(0).chain(KINDS.iter().map(|k| tags.iter().position(|t| t == k.tag).map(|i| i as u32).unwrap_or(0))).collect()
}

/// Un biome : ensembles de végétation (index dans vegetation::SETS, densité) et herbe du jeu (1 + index dans
/// KINDS, 0 aucune) ; l'herbe va sur les cases dont le matériau est dans `grass_on` (0 sable, 1 terre, 2 herbe,
/// 3 roche).
pub struct Biome {
    pub name: &'static str,
    pub tip: &'static str,
    pub sets: &'static [(usize, f32)],
    pub grass: u8,
    pub grass_on: &'static [u8],
}

/// Biomes — **estimé** (mélanges choisis d'après les régions de la carte d'origine).
pub const BIOMES: [Biome; 8] = [
    Biome { name: "Forêt de montagne", tip: "Pins et cèdres, buissons, rochers, flancs verts (Paleto, Chiliad)", sets: &[(0, 0.8), (4, 0.3), (6, 0.15)], grass: 7, grass_on: &[1, 2, 3] },
    Biome { name: "Forêt mixte", tip: "Pins, chênes, bouleaux, érables, buissons ; collines luxuriantes", sets: &[(12, 0.7), (4, 0.3)], grass: 6, grass_on: &[1, 2] },
    Biome { name: "Prairie et bosquets", tip: "Prairie fleurie, quelques feuillus et buissons", sets: &[(1, 0.2), (4, 0.25)], grass: 2, grass_on: &[1, 2] },
    Biome { name: "Bouleaux et rivière", tip: "Bouleaux, fougères et roseaux, berges", sets: &[(7, 0.7), (5, 0.3)], grass: 10, grass_on: &[0, 1, 2] },
    Biome { name: "Méditerranéen", tip: "Eucalyptus, cyprès, oliviers, mesquites ; collines sèches (Vinewood Hills)", sets: &[(8, 0.45), (4, 0.3), (6, 0.1)], grass: 8, grass_on: &[0, 1, 2, 3] },
    Biome { name: "Côte tropicale", tip: "Palmiers et plantes de bord de mer", sets: &[(2, 0.45), (4, 0.15)], grass: 9, grass_on: &[0, 1, 2] },
    Biome { name: "Désert", tip: "Arbres de Josué, cactus, buissons secs, herbes sèches (Sandy Shores)", sets: &[(3, 0.5), (9, 0.2), (6, 0.15)], grass: 5, grass_on: &[0, 1, 2] },
    Biome { name: "Neige", tip: "Sapins enneigés de North Yankton, rochers ; pas d'herbe", sets: &[(11, 0.6), (6, 0.1)], grass: 0, grass_on: &[] },
];

/// Rayon des disques de semis quand un biome peuple tout le terrain (m) — **estimé**.
const BIOME_DISC: f32 = 40.0;

impl DwApp {
    /// Interface : herbe du jeu (pinceau) et biomes (tout le terrain).
    pub fn grass_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Herbe du jeu").strong());
        ui.label(RichText::new("Comme sur la vraie carte : le jeu fait pousser lui-même l'herbe et les fleurs là où tu la peins (étiquette de végétation dans la collision du sol). Glisser : peindre · Ctrl + glisser : enlever.").small().weak());
        let ids = crate::grass::proc_ids(self.game.world.as_deref());
        egui::Grid::new("grasskinds").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, k) in KINDS.iter().enumerate() {
                let on = self.terrain.veg_mode == 1 && self.terrain.grass_kind as usize == i;
                let c = Color32::from_rgb(k.color[0], k.color[1], k.color[2]);
                let b = egui::Button::new(RichText::new(k.name).color(if on { Color32::BLACK } else { c })).min_size(egui::vec2(140.0, 24.0)).fill(if on { c } else { Color32::from_gray(40) });
                let found = ids.get(i + 1).copied().unwrap_or(0) > 0;
                let tip = if self.game.world.is_none() { format!("{} · charge le jeu pour trouver son étiquette", k.tip) } else if found { format!("{} · ProceduralID {}", k.tip, ids[i + 1]) } else { format!("{} · étiquette introuvable dans ce jeu", k.tip) };
                if ui.add(b).on_hover_text(tip).clicked() {
                    self.terrain.veg_mode = 1;
                    self.terrain.grass_kind = i as u8;
                    self.set_terrain_brush(crate::terrain::Brush::Vegetation);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.selectable_label(self.terrain.veg_mode == 0, "Pinceau : objets (arbres…)").clicked() {
                self.terrain.veg_mode = 0;
            }
            if ui.selectable_label(self.terrain.veg_mode == 1, "Pinceau : herbe du jeu").clicked() {
                self.terrain.veg_mode = 1;
                self.set_terrain_brush(crate::terrain::Brush::Vegetation);
            }
            if ui.button("Enlever toute l'herbe").clicked() {
                self.push_undo();
                if let Some(t) = self.proj.terrain.as_mut() {
                    t.grass.clear();
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Biomes").strong());
        ui.label(RichText::new("Un mélange de végétation et une herbe du jeu, semés sur tout le terrain (pas sur les routes, le béton ni l'eau).").small().weak());
        egui::Grid::new("biomes").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, b) in BIOMES.iter().enumerate() {
                let on = self.terrain.biome == i;
                let btn = egui::Button::new(RichText::new(b.name).color(if on { Color32::BLACK } else { Color32::from_rgb(170, 200, 255) })).min_size(egui::vec2(140.0, 24.0)).fill(if on { Color32::from_rgb(170, 200, 255) } else { Color32::from_gray(40) });
                if ui.add(btn).on_hover_text(b.tip).clicked() {
                    self.terrain.biome = i;
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        if ui.button(RichText::new(format!("Peupler tout le terrain : {}", BIOMES[self.terrain.biome.min(BIOMES.len() - 1)].name)).strong()).clicked() {
            self.status = self.apply_biome(self.terrain.biome);
        }
    }

    /// Sème un biome sur tout le terrain : herbe du jeu sur les cases qui s'y prêtent, puis chaque ensemble de
    /// végétation à sa densité (disques de BIOME_DISC m). Annulable (Ctrl+Z).
    pub fn apply_biome(&mut self, bi: usize) -> String {
        let b = &BIOMES[bi.min(BIOMES.len() - 1)];
        let Some(t) = self.proj.terrain.as_ref() else { return "Crée d'abord un terrain.".into() };
        let (o, cell, nx, ny) = (t.origin, t.cell, t.nx, t.ny);
        self.push_undo();
        let mut cells = 0;
        if let Some(t) = self.proj.terrain.as_mut() {
            let n = (nx - 1) * (ny - 1);
            if t.grass.len() != n {
                t.grass = vec![0; n];
            }
            for j in 0..ny - 1 {
                for i in 0..nx - 1 {
                    let idx = j * (nx - 1) + i;
                    let x = o.x + (i as f32 + 0.5) * cell;
                    let y = o.y + (j as f32 + 0.5) * cell;
                    let wet = t.height_at(x, y).map(|z| z < 0.3).unwrap_or(true);
                    if b.grass > 0 && !wet && b.grass_on.contains(&t.mat[idx]) {
                        t.grass[idx] = b.grass;
                        cells += 1;
                    }
                }
            }
        }
        let (old_set, old_d) = (self.terrain.veg_set, self.terrain.veg_density);
        let mut n = 0;
        let (w, h) = ((nx - 1) as f32 * cell, (ny - 1) as f32 * cell);
        for &(set, d) in b.sets {
            self.terrain.veg_set = set;
            self.terrain.veg_density = d;
            let mut y = BIOME_DISC;
            while y < h + BIOME_DISC {
                let mut x = BIOME_DISC;
                while x < w + BIOME_DISC {
                    n += self.veg_paint(v3(o.x + x.min(w), o.y + y.min(h), 0.0), BIOME_DISC * 1.05);
                    x += BIOME_DISC * 1.6;
                }
                y += BIOME_DISC * 1.6;
            }
        }
        self.terrain.veg_set = old_set;
        self.terrain.veg_density = old_d;
        self.terrain.dirty = true;
        format!("Biome « {} » : {n} objet(s) de végétation semé(s), herbe du jeu sur {cells} case(s). Ctrl+Z pour annuler.", b.name)
    }

    /// Dans la vue : touffes d'herbe du jeu près du centre de la vue (aperçu ; le jeu fait pousser la vraie).
    pub fn draw_grass_overlay(&self, painter: &egui::Painter) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        if t.grass.is_empty() || self.cam.map_view {
            return;
        }
        let c = self.cam.target;
        let reach = 90.0f32;
        let step = ((t.cell * 2.0).max(1.0) / t.cell).ceil().max(1.0) as usize;
        let (i0, i1) = ((((c.x - reach - t.origin.x) / t.cell).floor().max(0.0)) as usize, (((c.x + reach - t.origin.x) / t.cell).ceil().max(0.0) as usize).min(t.nx - 1));
        let (j0, j1) = ((((c.y - reach - t.origin.y) / t.cell).floor().max(0.0)) as usize, (((c.y + reach - t.origin.y) / t.cell).ceil().max(0.0) as usize).min(t.ny - 1));
        let mut drawn = 0;
        for j in (j0..j1).step_by(step) {
            for i in (i0..i1).step_by(step) {
                let g = t.grass_at(i, j);
                if g == 0 {
                    continue;
                }
                let k = &KINDS[(g as usize - 1).min(KINDS.len() - 1)];
                let x = t.origin.x + (i as f32 + 0.5) * t.cell;
                let y = t.origin.y + (j as f32 + 0.5) * t.cell;
                let Some(z) = t.height_at(x, y) else { continue };
                let col = Color32::from_rgb(k.color[0], k.color[1], k.color[2]);
                for (dx, dy) in [(-0.25f32, 0.0f32), (0.2, 0.15), (0.05, -0.2)] {
                    let base = v3(x + dx, y + dy, z);
                    if let (Some((a, _)), Some((b, _))) = (self.cam.project(base), self.cam.project(base + v3(dx * 0.4, dy * 0.4, 0.55))) {
                        painter.line_segment([a, b], egui::Stroke::new(1.5, col));
                    }
                }
                drawn += 1;
                if drawn > 5000 {
                    return;
                }
            }
        }
        let _ = V3::ZERO;
    }
}
