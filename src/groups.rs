//! Groupes : tes propres « bâtiments » faits d'objets choisis sur la carte (une station-service, un coin de
//! rue…), rangés autour d'un point d'ancrage au sol, puis posés ailleurs à la souris autant de fois qu'on veut.
//! Un groupe ne garde que les versions détaillées : à l'export, chaque copie d'un objet du jeu reçoit sa version
//! de loin (lods.rs, parent LOD du jeu) et sa collision, comme un bâtiment copié ; les lumières sont dans les
//! modèles du jeu ; un placement d'intérieur garde ses données (l'intérieur suit).
//! Mode « Copier » : l'original reste ; mode « Déplacer » : l'original est retiré de la carte (comme Suppr) à la
//! première pose.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::{Entity, MloInstData};
use eframe::egui;
use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};

/// Pas de rotation à la molette (degrés) : le cran de rotation de CodeWalker (RotationWidget::.ctor 0x060005b1 :
/// SnapAngleDegrees = 5).
pub const GROUP_WHEEL_STEP_DEG: f32 = 5.0;

/// Arrondi d'un angle au cran, comme CodeWalker (RotationWidget::SnapAngle 0x060005b5) :
/// a' = (a + s/2) − ((a + s/2) rem s), reste tronqué vers zéro comme en C#.
pub fn snap_angle(a: f32, s: f32) -> f32 {
    if s <= 0.0 {
        return a;
    }
    let b = a + s * 0.5;
    b - b % s
}
/// Pas de rotation avec Maj : un quart de tour (degrés).
pub const GROUP_QUARTER_DEG: f32 = 90.0;
/// Hauteur au-dessus du plus haut objet d'où l'on cherche le sol sous l'ancrage (m) — **estimé**.
pub const GROUP_GROUND_PROBE_UP: f32 = 50.0;
/// Pendant la pose : hauteur cherchée au-dessus et au-dessous du relief du jeu pour trouver la vraie surface de la
/// rue (m) — **estimé** (assez pour un trottoir ou une rue au-dessus du relief grossier, pas pour un toit).
pub const GROUP_STREET_PROBE: f32 = 3.0;
/// Nombre maximal de cadres verts dessinés pendant la pose — **estimé** (au-delà, seuls les modèles).
pub const GROUP_MAX_GHOST_BOXES: usize = 400;

/// Un objet du groupe, dans le repère du groupe (origine = point d'ancrage au sol, orientation 0).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupItem {
    pub archetype: String,
    pub pos: V3,
    pub rot: Quat,
    pub scale_xy: f32,
    pub scale_z: f32,
    pub lod_dist: f32,
    pub flags: u32,
    #[serde(default)]
    pub mlo: Option<MloInstData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub items: Vec<GroupItem>,
    /// identifiants des objets d'origine (mode « Déplacer » : retirés à la première pose)
    #[serde(default)]
    pub sources: Vec<u64>,
    /// taille de l'emprise (m)
    #[serde(default)]
    pub size: [f32; 3],
}

/// Pose en cours : le groupe suit la souris.
#[derive(Clone, Debug)]
pub struct GroupPlace {
    pub gi: usize,
    pub yaw_deg: f32,
    pub move_originals: bool,
    pub placed: usize,
}

/// Fenêtre « Faire un groupe » (nom demandé).
#[derive(Clone, Debug, Default)]
pub struct NewGroup {
    pub name: String,
    pub move_originals: bool,
}

/// Version détaillée (les versions de loin sont reprises du jeu à l'export, pas copiées dans le groupe).
pub fn is_detail_level(lod_level: &str) -> bool {
    lod_level.is_empty() || lod_level.contains("HD")
}

/// Position et orientation dans le monde d'un objet du groupe posé à `at`, tourné de `yaw` (radians) : même règle
/// que la rotation d'une sélection multiple de CodeWalker (MapSelection::SetRotation 0x06000471 : d = rot⁻¹ × neuve ;
/// position = centre + d × (position − centre), orientation = d × orientation).
pub fn item_world(it: &GroupItem, at: V3, yaw: f32) -> (V3, Quat) {
    let q = Quat::from_z(yaw);
    (at + q.rotate(it.pos), q.mul(it.rot).normalized())
}

/// Prochain angle à la molette : `dir` = +1 / −1 ; avec Maj, quart de tour aligné sur 0, 90, 180, 270.
pub fn next_yaw(yaw_deg: f32, dir: f32, quarter: bool) -> f32 {
    let y = if quarter {
        let q = GROUP_QUARTER_DEG;
        let base = (yaw_deg / q).round() * q;
        base + dir.signum() * q
    } else {
        snap_angle(yaw_deg + dir.signum() * GROUP_WHEEL_STEP_DEG, GROUP_WHEEL_STEP_DEG)
    };
    y.rem_euclid(360.0)
}

/// Point d'ancrage : centre de l'emprise (boîtes des objets) et hauteur donnée du sol ; positions relatives.
pub fn make_items(ents: &[(Entity, V3, V3)], mut ground: impl FnMut(f32, f32, f32) -> Option<f32>) -> Option<(V3, Vec<GroupItem>, [f32; 3])> {
    if ents.is_empty() {
        return None;
    }
    let big = 3.0e38f32;
    let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
    for (_, a, b) in ents {
        mn = mn.min(*a);
        mx = mx.max(*b);
    }
    // centre d'une sélection multiple de CodeWalker : moyenne des positions des objets
    // (MapSelection::SetMultipleSelectionItems 0x0600046f : somme des WidgetPosition × 1 / nombre)
    let k = 1.0 / ents.len() as f32;
    let (cx, cy) = (ents.iter().map(|(e, _, _)| e.pos.x).sum::<f32>() * k, ents.iter().map(|(e, _, _)| e.pos.y).sum::<f32>() * k);
    let lowest = ents.iter().map(|(e, _, _)| e.pos.z).fold(big, f32::min);
    let gz = ground(cx, cy, mx.z + GROUP_GROUND_PROBE_UP).filter(|z| *z > mn.z - 5.0 && *z < mx.z).unwrap_or(lowest);
    let anchor = v3(cx, cy, gz);
    let items = ents
        .iter()
        .map(|(e, _, _)| GroupItem { archetype: e.archetype.clone(), pos: e.pos - anchor, rot: e.rot, scale_xy: e.scale_xy, scale_z: e.scale_z, lod_dist: e.lod_dist, flags: e.flags, mlo: e.mlo.clone() })
        .collect();
    Some((anchor, items, [mx.x - mn.x, mx.y - mn.y, mx.z - mn.z]))
}

impl DwApp {
    /// Crée un groupe avec la sélection (versions détaillées seulement) ; rend un message.
    pub fn group_from_selection(&mut self, name: &str, move_originals: bool) -> String {
        let mut ents = Vec::new();
        let mut sources = Vec::new();
        let mut skipped = 0;
        for id in self.selected.clone() {
            let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)) else { continue };
            if !is_detail_level(&e.lod_level) {
                skipped += 1;
                continue;
            }
            let c = e.corners(&self.proj);
            let big = 3.0e38f32;
            let (mut a, mut b) = (v3(big, big, big), v3(-big, -big, -big));
            for p in c {
                a = a.min(p);
                b = b.max(p);
            }
            sources.push(e.id);
            ents.push((e, a, b));
        }
        let skip: std::collections::HashSet<u64> = self.sel_set.clone();
        let Some((_anchor, items, size)) = make_items(&ents, |x, y, z| self.ground_below(x, y, z, 400.0, &skip)) else {
            return "Choisis d'abord des objets (rectangle ou Ctrl + clic).".into();
        };
        let n = items.len();
        // nom libre dans le projet et dans la bibliothèque (sinon l'ancien fichier serait écrasé)
        self.group_lib_load();
        let taken: Vec<String> = self.proj.groups.iter().map(|g| g.name.to_lowercase()).chain(self.group_lib.items.iter().map(|it| it.group.name.to_lowercase())).collect();
        let name = unique_group_name(name, |c| taken.contains(&c.to_lowercase()));
        self.proj.groups.push(Group { name: name.clone(), items, sources, size });
        let gi = self.proj.groups.len() - 1;
        // aussi dans la bibliothèque « Mes groupes », commune à tous les projets
        let g = self.proj.groups[gi].clone();
        let saved = self.group_lib_save(&g).is_some();
        self.group_place = Some(GroupPlace { gi, yaw_deg: 0.0, move_originals, placed: 0 });
        self.tool = crate::app::Tool::Select;
        let lods = if skipped > 0 { format!(" ({skipped} version(s) de loin laissée(s) : le jeu les reprend à l'export)") } else { String::new() };
        let lib = if saved { " Enregistré dans « Mes groupes »." } else { "" };
        format!("Groupe « {name} » : {n} objet(s){lods}.{lib} Il suit la souris : molette = tourner, Maj + molette = quart de tour, clic = poser, Échap ou clic droit = fini.")
    }

    /// Pose le groupe en cours au point `at` ; rend les identifiants posés.
    pub fn group_place_at(&mut self, at: V3) -> Vec<u64> {
        let Some(gp) = self.group_place.clone() else { return Vec::new() };
        let Some(g) = self.proj.groups.get(gp.gi).cloned() else { return Vec::new() };
        self.push_undo();
        // « Déplacer » : l'original est retiré de la carte à la première pose
        let mut removed = 0;
        if gp.move_originals && gp.placed == 0 {
            let ids: std::collections::HashSet<u64> = g.sources.iter().copied().collect();
            removed = self.proj.delete_ids(&ids);
        }
        let yi = self.ensure_target_ymap();
        let yaw = gp.yaw_deg.to_radians();
        let mut ids = Vec::new();
        // chaque pose est un bloc : un clic prend tous ses objets (group_block)
        let inst = crate::model::new_id();
        self.group_open = None;
        for it in &g.items {
            let (p, q) = item_world(it, at, yaw);
            let mut e = Entity::new(&it.archetype, p);
            e.group_inst = inst;
            e.rot = q;
            e.scale_xy = it.scale_xy;
            e.scale_z = it.scale_z;
            e.lod_dist = it.lod_dist;
            e.flags = it.flags;
            e.mlo = it.mlo.clone();
            ids.push(e.id);
            self.proj.ymaps[yi].entities.push(e);
        }
        if let Some(p) = self.group_place.as_mut() {
            p.placed += 1;
        }
        self.set_selection(ids.clone());
        self.sync_game_project();
        let moved = if removed > 0 { format!(" ; {removed} objet(s) d'origine retiré(s) de la carte") } else { String::new() };
        self.status = format!("« {} » posé ({} fois) à {:.0} {:.0} {:.0}, tourné de {:.0}°{moved}. Clic pour en poser un autre, Échap pour finir.", g.name, gp.placed + 1, at.x, at.y, at.z, gp.yaw_deg);
        ids
    }

    /// Souris pendant la pose d'un groupe ; rend vrai si l'entrée est prise.
    pub fn group_input(&mut self, ctx: &egui::Context, resp: &egui::Response) -> bool {
        if self.group_place.is_none() {
            return false;
        }
        let (esc, scroll, shift) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::Escape), i.raw_scroll_delta.y + i.raw_scroll_delta.x, i.modifiers.shift));
        if esc || resp.clicked_by(egui::PointerButton::Secondary) {
            let n = self.group_place.as_ref().map(|g| g.placed).unwrap_or(0);
            self.group_place = None;
            self.status = format!("Pose du groupe finie ({n} posé(s)).");
            return true;
        }
        if resp.hovered() && scroll != 0.0 {
            if let Some(g) = self.group_place.as_mut() {
                g.yaw_deg = next_yaw(g.yaw_deg, scroll, shift);
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary) {
            if let Some(at) = self.mouse_ground {
                self.group_place_at(at);
            }
        }
        true
    }

    /// Le groupe en cours, là où il serait posé : (type, position, orientation, échelles) pour la vue.
    pub fn group_ghost(&self) -> Vec<(u32, V3, Quat, f32, f32)> {
        let (Some(gp), Some(at)) = (self.group_place.as_ref(), self.mouse_ground) else { return Vec::new() };
        let Some(g) = self.proj.groups.get(gp.gi) else { return Vec::new() };
        let yaw = gp.yaw_deg.to_radians();
        g.items
            .iter()
            .map(|it| {
                let (p, q) = item_world(it, at, yaw);
                (crate::render::arch_hash(&it.archetype), p, q, it.scale_xy, it.scale_z)
            })
            .collect()
    }

    /// Cadres verts du groupe en cours et point d'ancrage.
    pub fn draw_group_ghost(&self, painter: &egui::Painter) {
        let (Some(gp), Some(at)) = (self.group_place.as_ref(), self.mouse_ground) else { return };
        let Some(g) = self.proj.groups.get(gp.gi) else { return };
        let yaw = gp.yaw_deg.to_radians();
        for it in g.items.iter().take(GROUP_MAX_GHOST_BOXES) {
            let (p, q) = item_world(it, at, yaw);
            let mut e = Entity::new(&it.archetype, p);
            e.id = 0;
            e.rot = q;
            e.scale_xy = it.scale_xy;
            e.scale_z = it.scale_z;
            crate::render::draw_ghost(painter, &self.cam, &self.proj, &e);
        }
        if let Some((sp, _)) = self.cam.project(at) {
            let col = Color32::from_rgb(120, 255, 140);
            painter.circle_stroke(sp, 8.0, egui::Stroke::new(2.0_f32, col));
            painter.text(sp + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, format!("{} · {:.0}°", g.name, gp.yaw_deg), egui::FontId::proportional(13.0), col);
        }
    }

    /// Fenêtre « Faire un groupe » puis liste « Groupes du projet ».
    pub fn groups_window(&mut self, ctx: &egui::Context) {
        if let Some(mut ng) = self.group_new.take() {
            let mut open = true;
            let mut create = false;
            egui::Window::new("Faire un groupe").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.label(format!("{} objet(s) choisi(s). Nom du groupe :", self.selected.len()));
                let r = ui.text_edit_singleline(&mut ng.name);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
                ui.horizontal(|ui| {
                    ui.radio_value(&mut ng.move_originals, false, "Copier").on_hover_text("L'original reste en place");
                    ui.radio_value(&mut ng.move_originals, true, "Déplacer").on_hover_text("L'original est retiré de la carte du jeu à l'export (comme Suppr), dès la première pose");
                });
                ui.label(RichText::new("Les versions de loin, la collision et les lumières des objets du jeu suivent à l'export ; un intérieur garde son placement.").weak().small());
                if ui.button(RichText::new("Créer et poser").strong()).clicked() {
                    create = true;
                }
            });
            if create {
                self.status = self.group_from_selection(&ng.name, ng.move_originals);
            } else if open {
                self.group_new = Some(ng);
            }
        }
        if !self.groups_open {
            return;
        }
        self.group_lib_load();
        let mut open = true;
        // (groupe du projet, 0 copier / 1 déplacer / 2 supprimer)
        let mut action: Option<(usize, u8)> = None;
        // (groupe de la bibliothèque, 0 poser / 1 supprimer)
        let mut lib_action: Option<(usize, u8)> = None;
        let mut pick: Option<usize> = None;
        let mut preview: Option<(u32, String, Vec<(u32, V3, Quat, f32, f32)>)> = None;
        egui::Window::new("Mes groupes").open(&mut open).default_width(380.0).show(ctx, |ui| {
            ui.label(RichText::new("Dans ce projet").strong());
            if self.proj.groups.is_empty() {
                ui.label(RichText::new("Aucun groupe. Choisis des objets puis « Faire un groupe » dans l'onglet Sélection.").weak());
            }
            egui::ScrollArea::vertical().id_salt("grp_proj").max_height(150.0).show(ui, |ui| {
                for (gi, g) in self.proj.groups.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&g.name).strong());
                        ui.label(RichText::new(format!("{} objet(s), {:.0} × {:.0} m", g.items.len(), g.size[0], g.size[1])).weak());
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Poser une copie").clicked() {
                            action = Some((gi, 0));
                        }
                        if !g.sources.is_empty() && ui.button("Déplacer").on_hover_text("Retire l'original de la carte à la première pose").clicked() {
                            action = Some((gi, 1));
                        }
                        if ui.button("🗑").on_hover_text("Retirer le groupe du projet (ce qui est déjà posé reste ; il reste dans la bibliothèque)").clicked() {
                            action = Some((gi, 2));
                        }
                    });
                }
            });
            ui.separator();
            ui.label(RichText::new(format!("Bibliothèque (tous tes projets) : {} groupe(s)", self.group_lib.items.len())).strong());
            egui::ScrollArea::vertical().id_salt("grp_lib").max_height(260.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (li, it) in self.group_lib.items.iter().enumerate() {
                        let sel = self.group_lib.sel == Some(li);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(GROUP_THUMB_PX, GROUP_THUMB_PX + 16.0), egui::Sense::click());
                        let p = ui.painter_at(rect);
                        let img = egui::Rect::from_min_size(rect.min, egui::vec2(GROUP_THUMB_PX, GROUP_THUMB_PX));
                        p.rect_filled(img, 4.0, if sel { Color32::from_rgb(60, 70, 52) } else { Color32::from_rgb(34, 37, 42) });
                        if let Some(t) = &it.thumb {
                            crate::library::draw_thumb(&p, img.shrink(4.0), t);
                        }
                        p.text(egui::pos2(rect.center().x, rect.max.y - 8.0), egui::Align2::CENTER_CENTER, &it.group.name, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
                        if resp.clicked() {
                            pick = Some(li);
                        }
                        if resp.double_clicked() {
                            lib_action = Some((li, 0));
                        }
                        resp.on_hover_text(format!("{} : {} objet(s), {:.0} × {:.0} × {:.0} m · double-clic pour poser", it.group.name, it.group.items.len(), it.group.size[0], it.group.size[1], it.group.size[2]));
                    }
                });
            });
            if let Some(li) = self.group_lib.sel.filter(|&i| i < self.group_lib.items.len()) {
                let g = &self.group_lib.items[li].group;
                ui.separator();
                ui.label(RichText::new(&g.name).strong());
                preview = Some((crate::gta::crypto::joaat(&format!("dw_group_{}", g.name)), g.name.clone(), g.items.iter().map(|it| (crate::render::arch_hash(&it.archetype), it.pos, it.rot, it.scale_xy, it.scale_z)).collect()));
            }
        });
        // grand aperçu tournant (mêmes outils que la bibliothèque des bâtiments)
        if let Some((key, name, parts)) = preview {
            egui::Window::new("Aperçu du groupe").default_width(340.0).show(ctx, |ui| {
                self.textured_preview_with(ui, key, &name, &|w| crate::library::merged_tex_model(w, &parts));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Poser").strong()).on_hover_text("Le groupe suit la souris : molette = tourner, Maj + molette = quart de tour, clic = poser").clicked() {
                        lib_action = self.group_lib.sel.map(|i| (i, 0));
                    }
                    if ui.button("Supprimer de la bibliothèque").clicked() {
                        lib_action = self.group_lib.sel.map(|i| (i, 1));
                    }
                });
            });
        }
        self.groups_open = open;
        if let Some(li) = pick {
            self.group_lib.sel = Some(li);
        }
        match action {
            Some((gi, 2)) => {
                self.proj.groups.remove(gi);
                self.group_place = None;
            }
            Some((gi, m)) => {
                self.group_place = Some(GroupPlace { gi, yaw_deg: 0.0, move_originals: m == 1, placed: 0 });
                self.status = "Le groupe suit la souris : molette = tourner, Maj + molette = quart de tour, clic = poser, Échap = fini.".into();
            }
            None => {}
        }
        match lib_action {
            Some((li, 0)) => self.status = self.group_lib_place(li),
            Some((li, _)) => {
                if let Some(it) = self.group_lib.items.get(li) {
                    let _ = std::fs::remove_file(&it.path);
                }
                self.group_lib.loaded = false;
                self.group_lib.sel = None;
            }
            None => {}
        }
    }

    /// Lit la bibliothèque « Mes groupes » (une fois, puis après chaque changement).
    pub fn group_lib_load(&mut self) {
        if self.group_lib.loaded {
            return;
        }
        self.group_lib.loaded = true;
        self.group_lib.items = lib_dir().map(|d| read_lib(&d)).unwrap_or_default();
    }

    /// Enregistre un groupe dans la bibliothèque commune à tous les projets, avec sa vignette (vue de trois
    /// quarts des vrais modèles, comme la bibliothèque des bâtiments). Rend le chemin du fichier.
    pub fn group_lib_save(&mut self, g: &Group) -> Option<std::path::PathBuf> {
        let dir = lib_dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        let mut g = g.clone();
        // les objets d'origine n'existent que dans ce projet
        g.sources.clear();
        let thumb = self.game.world.as_ref().and_then(|w| {
            let parts: Vec<(u32, V3, Quat, f32, f32)> = g.items.iter().take(GROUP_THUMB_MAX_ITEMS).map(|it| (crate::render::arch_hash(&it.archetype), it.pos, it.rot, it.scale_xy, it.scale_z)).collect();
            crate::library::merged_tex_model(w, &parts).map(|m| crate::library::make_thumb_tex(&m, 0))
        });
        let file = LibFile {
            group: g.clone(),
            thumb: thumb.as_ref().map(|t| t.tris.iter().map(|(p, _)| [p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y]).collect()).unwrap_or_default(),
            cols: thumb.as_ref().map(|t| t.tris.iter().map(|(_, c)| [c[0].r(), c[0].g(), c[0].b(), c[1].r(), c[1].g(), c[1].b(), c[2].r(), c[2].g(), c[2].b()]).collect()).unwrap_or_default(),
        };
        let path = dir.join(format!("{}.json", safe_file_name(&g.name)));
        std::fs::write(&path, serde_json::to_string(&file).ok()?).ok()?;
        self.group_lib.loaded = false;
        Some(path)
    }

    /// Pose un groupe de la bibliothèque : il entre dans le projet (s'il n'y est pas déjà) et suit la souris.
    pub fn group_lib_place(&mut self, li: usize) -> String {
        let Some(it) = self.group_lib.items.get(li) else { return String::new() };
        let g = it.group.clone();
        let gi = match self.proj.groups.iter().position(|x| x.name == g.name && x.items.len() == g.items.len()) {
            Some(i) => i,
            None => {
                self.proj.groups.push(g.clone());
                self.proj.groups.len() - 1
            }
        };
        self.group_place = Some(GroupPlace { gi, yaw_deg: 0.0, move_originals: false, placed: 0 });
        format!("« {} » suit la souris : molette = tourner, Maj + molette = quart de tour, clic = poser, Échap = fini.", g.name)
    }

    /// Bloc d'un objet posé avec un groupe : tous les objets de la même pose ; un seul si ce bloc est « ouvert »
    /// (double-clic) ou si l'objet n'appartient à aucun groupe.
    pub fn group_block(&self, id: u64) -> Vec<u64> {
        let inst = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.group_inst).unwrap_or(0);
        if inst == 0 || self.group_open == Some(inst) {
            return vec![id];
        }
        let mut out = Vec::new();
        for y in &self.proj.ymaps {
            for e in &y.entities {
                if e.group_inst == inst {
                    out.push(e.id);
                }
            }
        }
        if out.is_empty() {
            out.push(id);
        }
        out
    }

    /// Double-clic sur un objet d'un bloc : ouvre le bloc pour modifier cette seule pièce.
    pub fn group_open_piece(&mut self, id: u64) -> bool {
        let inst = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.group_inst).unwrap_or(0);
        if inst == 0 {
            return false;
        }
        self.group_open = Some(inst);
        self.set_selection(vec![id]);
        let name = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()).unwrap_or_default();
        self.status = format!("Bloc ouvert : « {name} » seul est choisi (déplace-le, tourne-le, supprime-le). Clic hors du bloc pour le refermer.");
        true
    }
}

/// Côté d'une vignette de la bibliothèque (pixels) — **estimé**.
pub const GROUP_THUMB_PX: f32 = 96.0;
/// Objets pris pour la vignette (les premiers) — **estimé**, pour qu'un très grand groupe s'enregistre vite.
pub const GROUP_THUMB_MAX_ITEMS: usize = 150;

/// Bibliothèque « Mes groupes » en mémoire.
#[derive(Default)]
pub struct GroupLib {
    pub loaded: bool,
    pub items: Vec<LibEntry>,
    pub sel: Option<usize>,
}

pub struct LibEntry {
    pub path: std::path::PathBuf,
    pub group: Group,
    pub thumb: Option<crate::library::Thumb>,
}

/// Fichier d'un groupe de la bibliothèque : le groupe et sa vignette (triangles à l'écran, couleur à chaque coin).
#[derive(Serialize, Deserialize)]
struct LibFile {
    group: Group,
    #[serde(default)]
    thumb: Vec<[f32; 6]>,
    #[serde(default)]
    cols: Vec<[u8; 9]>,
}

/// Dossier de la bibliothèque, à côté des réglages de DragoWalker : %APPDATA%\DragoWalker\groupes.
pub fn lib_dir() -> Option<std::path::PathBuf> {
    if let Ok(d) = std::env::var("DW_GROUP_LIB") {
        return Some(std::path::PathBuf::from(d));
    }
    std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("DragoWalker").join("groupes"))
}

fn read_lib(dir: &std::path::Path) -> Vec<LibEntry> {
    let mut out = Vec::new();
    for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = f.path();
        if p.extension().map(|e| e == "json").unwrap_or(false) {
            let Ok(s) = std::fs::read_to_string(&p) else { continue };
            let Ok(lf) = serde_json::from_str::<LibFile>(&s) else { continue };
            let thumb = (!lf.thumb.is_empty()).then(|| {
                let size = lf.group.size;
                crate::library::Thumb {
                    tris: lf
                        .thumb
                        .iter()
                        .zip(lf.cols.iter().chain(std::iter::repeat(&[180u8; 9])))
                        .map(|(t, c)| {
                            ([egui::pos2(t[0], t[1]), egui::pos2(t[2], t[3]), egui::pos2(t[4], t[5])], [Color32::from_rgb(c[0], c[1], c[2]), Color32::from_rgb(c[3], c[4], c[5]), Color32::from_rgb(c[6], c[7], c[8])])
                        })
                        .collect(),
                    last: 0,
                    dims: v3(size[0], size[1], size[2]),
                }
            });
            out.push(LibEntry { path: p, group: lf.group, thumb });
        }
    }
    out.sort_by(|a, b| a.group.name.to_lowercase().cmp(&b.group.name.to_lowercase()));
    out
}

/// Nom d'un nouveau groupe, comme CodeWalker nomme une nouvelle carte (ProjectForm::NewYmap 0x06000d38 : i = 1,
/// « map » + i + « .ymap », i + 1 tant que le projet le contient déjà) : sans nom, « Groupe 1 », « Groupe 2 »… ;
/// un nom déjà pris reçoit le premier numéro libre à partir de 2.
pub fn unique_group_name(wanted: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = wanted.trim();
    if !base.is_empty() && !taken(base) {
        return base.to_string();
    }
    let (stem, mut i) = if base.is_empty() { ("Groupe", 1) } else { (base, 2) };
    loop {
        let c = format!("{stem} {i}");
        if !taken(&c) {
            return c;
        }
        i += 1;
    }
}

/// Nom de fichier sans caractères interdits par Windows.
pub fn safe_file_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() {
        "groupe".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_names_like_codewalker_new_map() {
        let taken = ["Groupe 1", "Garage", "Garage 2"];
        let t = |c: &str| taken.contains(&c);
        assert_eq!(unique_group_name("", t), "Groupe 2");
        assert_eq!(unique_group_name("Garage", t), "Garage 3");
        assert_eq!(unique_group_name(" Station ", t), "Station");
    }

    #[test]
    fn quarter_turns_snap_and_wheel_steps() {
        assert_eq!(next_yaw(0.0, 1.0, true), 90.0);
        assert_eq!(next_yaw(100.0, 1.0, true), 180.0);
        assert_eq!(next_yaw(0.0, -1.0, true), 270.0);
        assert_eq!(next_yaw(350.0, 1.0, false), 355.0);
        assert_eq!(next_yaw(357.0, 1.0, false), 0.0);
        assert_eq!(next_yaw(12.0, 1.0, false), 15.0);
        assert_eq!(snap_angle(7.4, 5.0), 5.0);
        assert_eq!(snap_angle(7.6, 5.0), 10.0);
    }

    #[test]
    fn anchor_at_ground_centre_and_rotation_keeps_layout() {
        let mut a = Entity::new("a", v3(10.0, 0.0, 5.0));
        a.rot = Quat::IDENTITY;
        let b = Entity::new("b", v3(20.0, 0.0, 5.0));
        let ents = vec![(a, v3(9.0, -1.0, 4.0), v3(11.0, 1.0, 8.0)), (b, v3(19.0, -1.0, 4.0), v3(21.0, 1.0, 8.0))];
        let (anchor, items, size) = make_items(&ents, |_, _, _| Some(4.5)).unwrap();
        assert!((anchor.x - 15.0).abs() < 1e-4 && (anchor.z - 4.5).abs() < 1e-4);
        assert!((size[0] - 12.0).abs() < 1e-4);
        // un quart de tour : les deux objets restent à 10 m l'un de l'autre, sur l'axe y
        let at = v3(0.0, 0.0, 0.0);
        let (p0, _) = item_world(&items[0], at, std::f32::consts::FRAC_PI_2);
        let (p1, _) = item_world(&items[1], at, std::f32::consts::FRAC_PI_2);
        assert!((p0 - p1).len() > 9.99 && (p0.x - p1.x).abs() < 1e-3);
        assert!((p0.z - 0.5).abs() < 1e-4);
    }
}
