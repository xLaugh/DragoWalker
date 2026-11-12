//! Bibliothèque des intérieurs : tous les intérieurs (MLO) du jeu, avec un plan vu de dessus, et deux façons
//! de les voir : « Aller voir » (on se téléporte dessus) ou « Voir isolé » (on se téléporte et on ne voit
//! que cet intérieur, sans le reste de la carte).

use crate::app::DwApp;
use crate::math::V3;
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke};

pub struct IntLib {
    pub open: bool,
    pub filter: String,
    pub chosen: Option<u32>,
    /// vignettes « maison de poupée » déjà faites, intérieurs sans modèle chargeable
    pub thumbs: std::collections::HashMap<u32, crate::library::Thumb>,
    pub missing: std::collections::HashSet<u32>,
    /// grande vue de l'intérieur choisi (plus de triangles)
    pub big: Option<(u32, Option<crate::library::Thumb>)>,
    pub frame: u64,
    /// fil qui fabrique les vignettes (un intérieur assemble des dizaines de modèles : trop long pour l'image)
    worker: Option<(std::sync::mpsc::Sender<ThumbJob>, std::sync::mpsc::Receiver<ThumbDone>)>,
    /// demandes envoyées au fil, pas encore revenues (intérieur, grande vue)
    pending: std::collections::HashSet<(u32, bool)>,
}

type Parts = Vec<(u32, V3, crate::math::Quat, f32, f32)>;

struct ThumbJob {
    arch: u32,
    big: bool,
    parts: Parts,
}

struct ThumbDone {
    arch: u32,
    big: bool,
    thumb: Option<crate::library::Thumb>,
    ms: f32,
}

/// Version des vignettes gardées sur le disque (à changer si leur fabrication change).
const THUMB_VERSION: u32 = 4;

/// Fichier d'une vignette gardée : %LOCALAPPDATA%\DragoWalker\cache\interieurs\v<version>_<hachage>[_g].bin.
fn cache_path(arch: u32, big: bool) -> Option<std::path::PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA"))?;
    Some(std::path::PathBuf::from(base).join("DragoWalker").join("cache").join("interieurs").join(format!("v{THUMB_VERSION}_{arch:08x}{}.bin", if big { "_g" } else { "" })))
}

/// Vignette en octets : « DWIT », nombre de triangles, puis pour chacun 3 points (x, y en f32) et 3 couleurs RGB.
fn thumb_to_bytes(t: &crate::library::Thumb) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + t.tris.len() * 33);
    b.extend_from_slice(b"DWIT");
    b.extend_from_slice(&(t.tris.len() as u32).to_le_bytes());
    for (p, c) in &t.tris {
        for q in p {
            b.extend_from_slice(&q.x.to_le_bytes());
            b.extend_from_slice(&q.y.to_le_bytes());
        }
        for k in c {
            b.extend_from_slice(&[k.r(), k.g(), k.b()]);
        }
    }
    b
}

fn thumb_from_bytes(b: &[u8]) -> Option<crate::library::Thumb> {
    if b.len() < 8 || &b[0..4] != b"DWIT" {
        return None;
    }
    let n = u32::from_le_bytes(b[4..8].try_into().ok()?) as usize;
    if b.len() != 8 + n * 33 {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let mut tris = Vec::with_capacity(n);
    for i in 0..n {
        let o = 8 + i * 33;
        let p = [Pos2::new(f(o), f(o + 4)), Pos2::new(f(o + 8), f(o + 12)), Pos2::new(f(o + 16), f(o + 20))];
        let c = |k: usize| Color32::from_rgb(b[o + 24 + k * 3], b[o + 25 + k * 3], b[o + 26 + k * 3]);
        tris.push((p, [c(0), c(1), c(2)]));
    }
    Some(crate::library::Thumb { tris, last: 0, dims: V3::ZERO })
}

/// Lance le fil des vignettes : il garde les dictionnaires de textures et les modèles déjà chargés (beaucoup
/// d'intérieurs partagent les mêmes meubles) ; la grande vue passe avant, puis la dernière vignette demandée
/// (celles qu'on voit après un défilement).
fn spawn_worker(src: crate::gta::loader::ModelSource) -> (std::sync::mpsc::Sender<ThumbJob>, std::sync::mpsc::Receiver<ThumbDone>) {
    let (tx, rx) = std::sync::mpsc::channel::<ThumbJob>();
    let (dtx, drx) = std::sync::mpsc::channel::<ThumbDone>();
    let _ = std::thread::Builder::new().name("vignettes-interieurs".into()).spawn(move || {
        let mut ytd = Vec::new();
        let mut models: std::collections::HashMap<u32, Option<std::sync::Arc<crate::library::TexModel>>> = Default::default();
        let mut queue: Vec<ThumbJob> = Vec::new();
        loop {
            if queue.is_empty() {
                match rx.recv() {
                    Ok(j) => queue.push(j),
                    Err(_) => return,
                }
            }
            while let Ok(j) = rx.try_recv() {
                queue.push(j);
            }
            let i = queue.iter().position(|j| j.big).unwrap_or(queue.len() - 1);
            let job = queue.remove(i);
            let t0 = std::time::Instant::now();
            // déjà faite lors d'une séance précédente
            if let Some(t) = cache_path(job.arch, job.big).and_then(|f| std::fs::read(f).ok()).and_then(|b| thumb_from_bytes(&b)) {
                if dtx.send(ThumbDone { arch: job.arch, big: job.big, thumb: Some(t), ms: t0.elapsed().as_secs_f32() * 1000.0 }).is_err() {
                    return;
                }
                continue;
            }
            if models.len() > 3000 {
                models.clear();
            }
            let m = crate::library::merged_tex_model_lite(&job.parts, &mut |h| models.entry(h).or_insert_with(|| crate::library::load_tex_model_src(&src, h, 32, 0, &mut ytd, true).map(std::sync::Arc::new)).clone());
            let thumb = m.map(|m| crate::library::make_thumb_cutaway(&m, 0, if job.big { INT_BIG_TRIS } else { INT_THUMB_TRIS }));
            if let (Some(t), Some(f)) = (&thumb, cache_path(job.arch, job.big)) {
                if let Some(d) = f.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = std::fs::write(f, thumb_to_bytes(t));
            }
            if dtx.send(ThumbDone { arch: job.arch, big: job.big, thumb, ms: t0.elapsed().as_secs_f32() * 1000.0 }).is_err() {
                return;
            }
        }
    });
    (tx, drx)
}

/// Triangles gardés dans une vignette d'intérieur, et dans la grande vue — **estimé** (assez pour reconnaître
/// les pièces sans ralentir la fenêtre).
const INT_THUMB_TRIS: usize = 3500;
const INT_BIG_TRIS: usize = 14000;
/// Vignettes gardées en mémoire au plus.
const INT_THUMB_CACHE: usize = 400;

impl IntLib {
    pub fn new(open: bool, filter: String) -> IntLib {
        IntLib { open, filter, chosen: std::env::var("DW_INTLIB_CHOSEN").ok().map(|n| crate::gta::crypto::joaat(&n.to_lowercase())), thumbs: Default::default(), missing: Default::default(), big: None, frame: 0, worker: None, pending: Default::default() }
    }
}

/// Un type d'intérieur et ses placements dans la carte.
struct IntEntry {
    arch: u32,
    name: String,
    places: Vec<u32>,
    objects: usize,
    sets: usize,
}

impl DwApp {
    fn int_entries(&self) -> Vec<IntEntry> {
        let Some(w) = self.game.world.as_ref() else { return Vec::new() };
        let f = self.intlib.filter.to_lowercase();
        let mut map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        for (i, e) in w.ents.iter().enumerate() {
            if e.mlo && w.ymaps[e.ymap as usize].visible {
                map.entry(e.arch).or_default().push(i as u32);
            }
        }
        let mut v: Vec<IntEntry> = map
            .into_iter()
            .filter_map(|(arch, places)| {
                let name = w.name_of(arch);
                if !f.is_empty() && !name.to_lowercase().contains(&f) {
                    return None;
                }
                let def = w.mlo_defs.get(&arch);
                Some(IntEntry { arch, name, places, objects: def.map(|d| d.children.len()).unwrap_or(0), sets: def.map(|d| d.sets.len()).unwrap_or(0) })
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Placement le plus proche du centre de la vue.
    fn nearest_place(&self, places: &[u32]) -> Option<u32> {
        let w = self.game.world.as_ref()?;
        let t = self.cam.target;
        places.iter().copied().min_by(|a, b| w.ents[*a as usize].pos.dist2d(t).partial_cmp(&w.ents[*b as usize].pos.dist2d(t)).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Demande au fil la vignette « maison de poupée » d'un type d'intérieur (ou sa grande vue) : ses objets fixes,
    /// sans les variantes, assemblés.
    fn request_int_thumb(&mut self, arch: u32, big: bool) {
        if self.intlib.pending.contains(&(arch, big)) {
            return;
        }
        let Some(w) = self.game.world.as_ref() else { return };
        let Some(def) = w.mlo_defs.get(&arch) else {
            if big {
                self.intlib.big = Some((arch, None));
            } else {
                self.intlib.missing.insert(arch);
            }
            return;
        };
        // mêmes objets que la vue 3D (render.rs) : sans les « proxies » (RenderIsEntityFinalRender 0x06000991) ni
        // les modèles cachés (proxy, _occl, _refl), et sans les variantes
        let parts: Parts = def
            .children
            .iter()
            .filter(|c| c.set < 0)
            .filter(|c| !matches!(c.flags, 35127296 | 39321602 | 135790592 | 135790593 | 536870912 | 672661504))
            .filter(|c| !w.archetypes.get(&c.arch).map(|a| a.flags & 2048 != 0).unwrap_or(false) && !w.hidden_archs.contains(&c.arch))
            .map(|c| (c.arch, c.pos, c.rot, c.sxy, c.sz))
            .collect();
        if self.intlib.worker.is_none() {
            self.intlib.worker = Some(spawn_worker(w.model_source()));
        }
        if let Some((tx, _)) = &self.intlib.worker {
            if tx.send(ThumbJob { arch, big, parts }).is_ok() {
                self.intlib.pending.insert((arch, big));
            }
        }
    }

    /// Vignettes revenues du fil.
    fn receive_int_thumbs(&mut self) {
        let Some((_, rx)) = &self.intlib.worker else { return };
        let done: Vec<ThumbDone> = rx.try_iter().collect();
        for d in done {
            self.intlib.pending.remove(&(d.arch, d.big));
            if std::env::var("DW_DEBUG_INTTHUMB").is_ok() {
                println!("vignette d'intérieur {}{} : {:.0} ms", self.game.world.as_ref().map(|w| w.name_of(d.arch)).unwrap_or_default(), if d.big { " (grande)" } else { "" }, d.ms);
            }
            if d.big {
                if self.intlib.chosen == Some(d.arch) {
                    self.intlib.big = Some((d.arch, d.thumb));
                }
            } else {
                match d.thumb {
                    Some(mut t) => {
                        t.last = self.intlib.frame;
                        self.intlib.thumbs.insert(d.arch, t);
                    }
                    None => {
                        self.intlib.missing.insert(d.arch);
                    }
                }
            }
        }
    }

    /// Image de l'intérieur dans le cadre : sa vignette si elle est prête (sinon elle est demandée), à défaut le plan.
    fn draw_int_image(&mut self, painter: &egui::Painter, rect: egui::Rect, arch: u32, todo: &mut Vec<u32>) {
        let frame = self.intlib.frame;
        if let Some(t) = self.intlib.thumbs.get_mut(&arch) {
            t.last = frame;
            crate::library::draw_thumb(painter, rect, t);
        } else if self.intlib.missing.contains(&arch) {
            self.draw_int_plan(painter, rect, arch);
        } else {
            todo.push(arch);
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), Color32::GRAY);
        }
    }

    /// Plan vu de dessus : les objets de l'intérieur en points, dans le cadre.
    fn draw_int_plan(&self, painter: &egui::Painter, rect: egui::Rect, arch: u32) {
        let Some(w) = self.game.world.as_ref() else { return };
        let Some(def) = w.mlo_defs.get(&arch) else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "pas de plan", egui::FontId::proportional(10.0), Color32::GRAY);
            return;
        };
        if def.children.is_empty() {
            return;
        }
        let (mut mn, mut mx) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
        for c in &def.children {
            mn.x = mn.x.min(c.pos.x);
            mn.y = mn.y.min(c.pos.y);
            mx.x = mx.x.max(c.pos.x);
            mx.y = mx.y.max(c.pos.y);
        }
        let sx = (mx.x - mn.x).max(1.0);
        let sy = (mx.y - mn.y).max(1.0);
        let s = (rect.width() - 10.0) / sx.max(sy);
        let c0 = rect.center();
        let mid = Pos2::new((mn.x + mx.x) * 0.5, (mn.y + mx.y) * 0.5);
        for c in &def.children {
            let p = Pos2::new(c0.x + (c.pos.x - mid.x) * s, c0.y - (c.pos.y - mid.y) * s);
            let (mn3, mx3) = w.arch_box(c.arch);
            let size = ((mx3 - mn3).len() * s * 0.25).clamp(1.0, 6.0);
            let col = if c.set >= 0 { Color32::from_rgb(255, 170, 60) } else { Color32::from_rgb(110, 190, 255) };
            painter.circle_filled(p, size, col.gamma_multiply(0.75));
        }
    }

    /// Fenêtre « Bibliothèque des intérieurs ».
    pub fn intlib_window(&mut self, ctx: &egui::Context) {
        // bandeau quand un intérieur est vu seul (une seule fois par image)
        if let Some(iso) = self.game.isolate.filter(|_| self.dock_phase == 1) {
            let name = self.game.world.as_ref().map(|w| w.name_of(w.ents[iso as usize].arch)).unwrap_or_default();
            egui::Area::new(egui::Id::new("iso_banner")).fixed_pos(self.cam.rect.center_top() + egui::vec2(-160.0, 8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Vu seul : {name}")).strong().color(Color32::from_rgb(255, 200, 90)));
                        if ui.button("Revenir à la carte (Échap)").clicked() {
                            self.game.isolate = None;
                        }
                    });
                });
            });
        }
        if !self.intlib.open {
            return;
        }
        let mut open = self.intlib.open;
        let mut go: Option<(u32, bool)> = None;
        self.show_panel(ctx, "interieurs", "Bibliothèque des intérieurs", crate::docking::Dock::Floating, [860.0, 600.0], &mut open, |s, ui| {
            if s.game.world.is_none() {
                ui.label("Charge d'abord le jeu (onglet Jeu).");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut s.intlib.filter).hint_text("Chercher : garage, apartment, office, bank…").desired_width(260.0));
            });
            let list = s.int_entries();
            s.intlib.frame += 1;
            let mut todo: Vec<u32> = Vec::new();
            ui.label(RichText::new(format!("{} intérieurs · vue en coupe, sans plafond ni murs côté caméra · « Aller voir » ou « Voir isolé »", list.len())).small().weak());
            ui.separator();
            let avail = ui.available_size();
            let side = 260.0f32.min(avail.x * 0.35);
            let grid_w = (avail.x - side - 16.0).max(200.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(grid_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(grid_w);
                    let cell = 120.0;
                    let cols = ((grid_w / (cell + 8.0)).floor() as usize).max(1);
                    let rows = list.len().div_ceil(cols);
                    egui::ScrollArea::vertical().id_salt("intlib_grid").auto_shrink([false; 2]).show_rows(ui, cell + 26.0, rows, |ui, range| {
                        for r in range {
                            ui.horizontal(|ui| {
                                for c in 0..cols {
                                    let i = r * cols + c;
                                    if i >= list.len() {
                                        break;
                                    }
                                    let e = &list[i];
                                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell + 4.0, cell + 22.0), Sense::click());
                                    let img = egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2(cell, cell));
                                    let p = ui.painter_at(rect);
                                    p.rect_filled(img, 5.0, if resp.hovered() { Color32::from_rgb(44, 50, 60) } else { Color32::from_rgb(30, 33, 39) });
                                    s.draw_int_image(&p, img.shrink(4.0), e.arch, &mut todo);
                                    if s.intlib.chosen == Some(e.arch) {
                                        p.rect_stroke(img, 5.0, Stroke::new(2.0_f32, Color32::from_rgb(255, 170, 60)), egui::StrokeKind::Inside);
                                    }
                                    let short: String = if e.name.chars().count() > 20 { format!("{}…", e.name.chars().take(19).collect::<String>()) } else { e.name.clone() };
                                    p.text(egui::pos2(img.center().x, img.max.y + 3.0), egui::Align2::CENTER_TOP, short, egui::FontId::proportional(11.0), Color32::from_gray(200));
                                    let resp = resp.on_hover_text(&e.name);
                                    if resp.clicked() {
                                        s.intlib.chosen = Some(e.arch);
                                    }
                                    if resp.double_clicked() {
                                        if let Some(pl) = s.nearest_place(&e.places) {
                                            go = Some((pl, true));
                                        }
                                    }
                                }
                            });
                        }
                    });
                });
                // vignettes à préparer (dans le fil) ; on redessine tant qu'il en reste en route
                for arch in todo.drain(..) {
                    s.request_int_thumb(arch, false);
                }
                s.receive_int_thumbs();
                if !s.intlib.pending.is_empty() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
                if s.intlib.thumbs.len() > INT_THUMB_CACHE {
                    let mut v: Vec<(u64, u32)> = s.intlib.thumbs.iter().map(|(k, t)| (t.last, *k)).collect();
                    v.sort();
                    for (_, k) in v.into_iter().take(s.intlib.thumbs.len() - INT_THUMB_CACHE) {
                        s.intlib.thumbs.remove(&k);
                    }
                }
                ui.vertical(|ui| {
                    ui.set_width(side);
                    let Some(arch) = s.intlib.chosen else {
                        ui.label("Choisis un intérieur (double-clic : le voir isolé).");
                        return;
                    };
                    let Some(e) = list.iter().find(|e| e.arch == arch) else { return };
                    ui.label(RichText::new(&e.name).strong());
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 6.0, Color32::from_rgb(28, 31, 36));
                    // grande vue : faite à part, avec plus de triangles ; la vignette en attendant
                    if s.intlib.big.as_ref().map(|b| b.0) != Some(arch) {
                        s.request_int_thumb(arch, true);
                    }
                    match s.intlib.big.as_ref().filter(|b| b.0 == arch).map(|b| b.1.as_ref()) {
                        Some(Some(t)) => crate::library::draw_thumb(&p, rect.shrink(6.0), t),
                        Some(None) => s.draw_int_plan(&p, rect.shrink(10.0), arch),
                        None => match s.intlib.thumbs.get(&arch) {
                            Some(t) => crate::library::draw_thumb(&p, rect.shrink(6.0), t),
                            None => {
                                p.text(rect.center(), egui::Align2::CENTER_CENTER, "préparation de la vue…", egui::FontId::proportional(13.0), Color32::GRAY);
                            }
                        },
                    }
                    ui.label(format!("{} placement(s) dans la carte · {} objet(s) · {} variante(s)", e.places.len(), e.objects, e.sets));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Aller voir").on_hover_text("Se téléporter sur l'intérieur (le plus proche s'il y en a plusieurs)").clicked() {
                            if let Some(pl) = s.nearest_place(&e.places) {
                                go = Some((pl, false));
                            }
                        }
                        if ui.button(RichText::new("Voir isolé").strong()).on_hover_text("Se téléporter et ne voir que cet intérieur").clicked() {
                            if let Some(pl) = s.nearest_place(&e.places) {
                                go = Some((pl, true));
                            }
                        }
                    });
                });
            });
        });
        self.intlib.open = open;
        if let Some((pl, iso)) = go {
            let pos: Option<V3> = self.game.world.as_ref().map(|w| w.ents[pl as usize].pos);
            if let Some(p) = pos {
                // centre de l'intérieur : moyenne de ses objets
                let c = self
                    .game
                    .world
                    .as_ref()
                    .and_then(|w| {
                        let e = &w.ents[pl as usize];
                        let d = w.mlo_defs.get(&e.arch)?;
                        if d.children.is_empty() {
                            return None;
                        }
                        let s = d.children.iter().fold(V3::ZERO, |a, k| a + e.pos + e.rot().rotate(k.pos));
                        Some(s * (1.0 / d.children.len() as f32))
                    })
                    .unwrap_or(p);
                self.cam.target = c;
                self.cam.dist = 22.0;
                self.game.interiors = true;
                self.game.isolate = if iso { Some(pl) } else { None };
                let name = self.game.world.as_ref().map(|w| w.name_of(w.ents[pl as usize].arch)).unwrap_or_default();
                self.status = if iso { format!("« {name} » vu seul. Échap ou « Revenir à la carte » pour tout réafficher.") } else { format!("Téléporté sur « {name} ».") };
            }
        }
    }
}
