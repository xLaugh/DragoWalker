//! Fiabilité : ne pas perdre le travail et savoir pourquoi ça a planté.
//!
//! - Annulation : l'historique est borné en mémoire (budget) en plus du nombre de pas ; les gros tableaux du
//!   terrain sont partagés entre les pas (copie à l'écriture).
//! - Sauvegarde automatique toutes les 2 minutes (en arrière-plan) dans %LOCALAPPDATA%\DragoWalker ; au
//!   démarrage suivant, si elle existe encore (arrêt anormal), l'app propose de récupérer le projet.
//! - Journal d'erreurs : une erreur interne (panic) est écrite dans crash.log ; au démarrage suivant, l'app
//!   montre le rapport (à copier ou à envoyer).
//! - Fermeture : si le projet n'est pas enregistré, l'app demande quoi faire.
//! Un fichier verrou (ouvert sans partage) signale un arrêt brutal et évite que deux fenêtres DragoWalker
//! se marchent dessus. Rien de tout cela en build debug, en mode capture ou avec une variable DW_* (tests),
//! sauf DW_SAFETY_TEST=1 ; DW_SAFETY_DIR change le dossier, DW_SAFETY_SECS l'intervalle de sauvegarde.

use crate::app::DwApp;
use crate::model::Project;
use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Demande une sauvegarde immédiate (mise à jour de l'app : le projet est gardé pour le redémarrage).
pub static AUTOSAVE_NOW: AtomicBool = AtomicBool::new(false);
static LOCK: Mutex<Option<std::fs::File>> = Mutex::new(None);

const AUTOSAVE_EVERY: f64 = 120.0;
const UNDO_MAX_STEPS: usize = 100;
const UNDO_MIN_STEPS: usize = 5;
const UNDO_BUDGET: usize = 600 << 20;
const LOG_MAX: u64 = 256 * 1024;

// ------------------------------------------------------------------ dossier et mode

pub fn active() -> bool {
    if std::env::var_os("DW_SAFETY_TEST").is_some() {
        return true;
    }
    !(cfg!(debug_assertions) || std::env::vars().any(|(k, _)| k.starts_with("DW_")))
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("DW_SAFETY_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("DragoWalker")
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Date UTC « 2026-11-12 14:03:11 » (algorithme de calendrier grégorien de H. Hinnant).
pub fn utc_string(unix: u64) -> String {
    let days = (unix / 86400) as i64;
    let s = unix % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

// ------------------------------------------------------------------ annulation bornée

fn light_bytes(p: &Project) -> usize {
    let ent = |n: usize| n * 380;
    let mut b = 64 * 1024;
    for y in &p.ymaps {
        b += ent(y.entities.len());
    }
    for t in &p.ytyps {
        b += t.raw_archetypes.iter().map(|s| s.len() + 24).sum::<usize>();
        for m in &t.mlos {
            b += ent(m.entities.len());
        }
    }
    if let Some(t) = &p.terrain {
        b += t.roads.len() * 4000;
    }
    b
}

fn shared_arrays(p: &Project) -> Vec<(usize, usize)> {
    match &p.terrain {
        Some(t) => vec![
            (Arc::as_ptr(&t.h) as *const () as usize, t.h.len() * 4),
            (Arc::as_ptr(&t.mat) as *const () as usize, t.mat.len()),
            (Arc::as_ptr(&t.grass) as *const () as usize, t.grass.len()),
        ],
        None => vec![],
    }
}

/// Mémoire (approximative) que l'historique ajoute à celle du projet actuel : les tableaux du terrain partagés
/// ne sont comptés qu'une fois.
pub fn undo_bytes(undo: &[Project], current: &Project) -> usize {
    let mut seen: HashSet<usize> = shared_arrays(current).into_iter().map(|a| a.0).collect();
    let mut total = 0;
    for p in undo {
        total += light_bytes(p);
        for (ptr, len) in shared_arrays(p) {
            if seen.insert(ptr) {
                total += len;
            }
        }
    }
    total
}

/// Retire les plus vieux pas : au plus 100, et pas plus de 600 Mo (en gardant au moins 5 pas).
pub fn trim_undo(undo: &mut Vec<Project>, current: &Project) {
    let extra = undo.len().saturating_sub(UNDO_MAX_STEPS);
    if extra > 0 {
        undo.drain(..extra);
    }
    if undo.len() > UNDO_MIN_STEPS && undo_bytes(undo, current) > UNDO_BUDGET {
        // plus vieux d'abord ; on recompte par paquets pour rester rapide
        while undo.len() > UNDO_MIN_STEPS && undo_bytes(undo, current) > UNDO_BUDGET {
            let n = (undo.len() / 8).max(1).min(undo.len() - UNDO_MIN_STEPS);
            undo.drain(..n);
        }
    }
}

// ------------------------------------------------------------------ sauvegarde automatique
//
// Chaque fenêtre DragoWalker écrit dans son propre emplacement (« autosave-<n° de processus>-<heure> ») : une
// sauvegarde retrouvée qui attend la décision de la personne, ou celle d'une autre fenêtre, n'est jamais
// écrasée. Au démarrage, la fenêtre principale propose de récupérer chaque emplacement resté là (le plus récent
// d'abord) ; l'ancien nom « autosave » (versions précédentes) est aussi reconnu.

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Recovery {
    pub name: String,
    pub when: u64,
    pub original: Option<String>,
    pub version: String,
    /// emplacement de la sauvegarde (nom des fichiers sans extension), rempli à la lecture
    #[serde(skip)]
    pub slot: String,
}

fn autosave_file(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.dwproj"))
}
fn autosave_meta(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.json"))
}

/// Fichier temporaire d'une écriture atomique : à côté du fichier, extension « .tmp » ajoutée (pas remplacée,
/// pour que « x.dwproj » et « x.json » n'aient pas le même).
fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Écrit un fichier sans jamais laisser une version à moitié écrite : écriture complète dans un fichier
/// temporaire (forcée sur le disque), puis remplacement du fichier en une seule opération.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = tmp_path(path);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("écriture de {} : {e}", tmp.display()));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("remplacement de {} : {e}", path.display())
    })
}

/// Écrit le projet (sans la carte d'exemple) puis ses informations ; les informations en dernier : sans elles,
/// la sauvegarde est ignorée (écriture interrompue).
pub fn write_autosave(dir: &Path, slot: &str, mut p: Project, original: Option<String>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    p.ymaps.retain(|y| !y.sample);
    crate::files::sanitize_project(&mut p);
    let data = serde_json::to_vec(&p).map_err(|e| e.to_string())?;
    write_atomic(&autosave_file(dir, slot), &data)?;
    let rec = Recovery { name: p.name.clone(), when: now_unix(), original, version: env!("CARGO_PKG_VERSION").to_string(), slot: slot.to_string() };
    write_atomic(&autosave_meta(dir, slot), &serde_json::to_vec(&rec).map_err(|e| e.to_string())?)
}

pub fn read_autosave_info(dir: &Path, slot: &str) -> Option<Recovery> {
    let mut rec: Recovery = serde_json::from_slice(&std::fs::read(autosave_meta(dir, slot)).ok()?).ok()?;
    rec.slot = slot.to_string();
    autosave_file(dir, slot).is_file().then_some(rec)
}

/// Toutes les sauvegardes automatiques présentes dans le dossier, la plus récente d'abord.
pub fn list_autosaves(dir: &Path) -> Vec<Recovery> {
    let mut out: Vec<Recovery> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let slot = name.strip_suffix(".json")?;
                    if slot == "autosave" || slot.starts_with("autosave-") {
                        read_autosave_info(dir, slot)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.slot.cmp(&b.slot)));
    out
}

pub fn read_autosave(dir: &Path, slot: &str) -> Option<String> {
    std::fs::read_to_string(autosave_file(dir, slot)).ok()
}

pub fn clear_autosave(dir: &Path, slot: &str) {
    for p in [autosave_meta(dir, slot), autosave_file(dir, slot)] {
        let _ = std::fs::remove_file(tmp_path(&p));
        let _ = std::fs::remove_file(&p);
    }
}

/// Numéro de génération des sauvegardes automatiques : changé quand le projet est enregistré (ou fermé) ; une
/// écriture en arrière-plan commencée avant efface ce qu'elle vient d'écrire, sinon elle ressusciterait une
/// sauvegarde devenue inutile.
static GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GEN_LOCK: Mutex<()> = Mutex::new(());
/// sauvegarde automatique en service dans cette fenêtre
static AUTOSAVE_ON: AtomicBool = AtomicBool::new(false);

fn bump_gen_and_clear(dir: &Path, slot: &str) {
    let _g = GEN_LOCK.lock();
    GEN.fetch_add(1, Ordering::SeqCst);
    clear_autosave(dir, slot);
}

/// Écriture faite par le fil d'arrière-plan : si le projet a été enregistré entre-temps, elle est retirée.
fn autosave_job(dir: &Path, slot: &str, gen: u64, p: Project, original: Option<String>) -> Result<(), String> {
    let r = write_autosave(dir, slot, p, original);
    let _g = GEN_LOCK.lock();
    if GEN.load(Ordering::SeqCst) != gen {
        clear_autosave(dir, slot);
    }
    r
}

// ------------------------------------------------------------------ journal d'erreurs

pub fn format_report(version: &str, unix: u64, thread: &str, msg: &str, loc: &str) -> String {
    format!(
        "DragoWalker {version} — erreur interne\nDate : {} UTC\nFil : {thread}\nMessage : {msg}\nEndroit : {loc}\nSystème : {} {}\n",
        utc_string(unix),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Enlève le nom d'utilisateur Windows des chemins d'un rapport (il peut être envoyé à d'autres).
fn anonymize(s: &str) -> String {
    match std::env::var("USERPROFILE") {
        Ok(h) if h.len() > 3 => s.replace(&h, "~").replace(&h.replace('\\', "/"), "~"),
        _ => s.to_string(),
    }
}

fn append(path: &Path, text: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
    }
}

/// À appeler une fois au démarrage : toute erreur interne est écrite dans crash.log et dans crash_pending.txt
/// (montré au démarrage suivant).
pub fn install_panic_hook() {
    if !active() {
        return;
    }
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    // journal trop gros : on garde la fin
    let log = dir.join("crash.log");
    if let Ok(meta) = std::fs::metadata(&log) {
        if meta.len() > LOG_MAX {
            if let Ok(s) = std::fs::read(&log) {
                let keep = &s[s.len() - (LOG_MAX as usize / 2)..];
                let _ = std::fs::write(&log, keep);
            }
        }
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(sans message)".into());
        let loc = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("sans nom").to_string();
        let report = anonymize(&format_report(env!("CARGO_PKG_VERSION"), now_unix(), &thread, &msg, &loc));
        append(&dir.join("crash.log"), &format!("{report}\n"));
        append(&dir.join("crash_pending.txt"), &format!("{report}\n"));
        prev(info);
    }));
}

// ------------------------------------------------------------------ état

/// Ouverture qui attend la réponse à « Enregistrer les modifications ? ».
#[derive(Clone, Debug)]
pub enum PendingOpen {
    File(PathBuf),
    Recovery(Recovery),
}

pub struct Safety {
    /// sauvegarde automatique en service (toutes les fenêtres, chacune dans son emplacement)
    pub enabled: bool,
    /// cette fenêtre tient le verrou : elle propose la récupération et montre le rapport d'erreur
    pub main: bool,
    pub dirty: bool,
    pub rev: u64,
    auto_rev: u64,
    next_auto: f64,
    every: f64,
    busy: Arc<AtomicBool>,
    /// résultat de la dernière écriture en arrière-plan (numéro de modification sauvé, ou l'erreur)
    result: Arc<Mutex<Option<Result<u64, String>>>>,
    /// dernière erreur de sauvegarde automatique (montrée dans la barre d'état ; nouvel essai dans 30 s)
    pub auto_err: Option<String>,
    /// emplacement de la sauvegarde automatique de cette fenêtre
    slot: String,
    /// sauvegardes récupérées : effacées dès que le projet récupéré est sauvegardé ou enregistré
    stale: Vec<String>,
    pub recovery: Option<Recovery>,
    /// projet à ouvrir après la question « Enregistrer les modifications ? »
    pub pending_open: Option<PendingOpen>,
    /// rapport d'erreur de la session précédente
    pub crash: Option<String>,
    /// la session précédente ne s'est pas fermée normalement
    pub unexpected: bool,
    pub confirm_quit: bool,
    pub allow_close: bool,
    test_touch: bool,
    test_panic: Option<u64>,
    test_recover: bool,
}

impl Safety {
    pub fn new(enabled: bool) -> Safety {
        let mut s = Safety {
            enabled: false,
            main: false,
            dirty: false,
            rev: 0,
            auto_rev: 0,
            next_auto: AUTOSAVE_EVERY,
            every: std::env::var("DW_SAFETY_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(AUTOSAVE_EVERY),
            busy: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(None)),
            auto_err: None,
            slot: format!("autosave-{}-{}", std::process::id(), now_unix()),
            stale: Vec::new(),
            recovery: None,
            pending_open: None,
            crash: None,
            unexpected: false,
            confirm_quit: false,
            allow_close: false,
            test_touch: std::env::var_os("DW_TEST_TOUCH").is_some(),
            test_panic: std::env::var("DW_TEST_PANIC").ok().and_then(|v| v.parse().ok()),
            test_recover: std::env::var_os("DW_TEST_RECOVER").is_some(),
        };
        s.next_auto = s.every;
        if !enabled || !active() {
            return s;
        }
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        // fenêtres suivantes : sauvegarde automatique (dans leur emplacement) et question à la fermeture, mais la
        // récupération et le rapport d'erreur restent à la fenêtre qui tient le verrou
        s.enabled = true;
        AUTOSAVE_ON.store(true, Ordering::SeqCst);
        let lock = dir.join("running.lock");
        let existed = lock.exists();
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.share_mode(0);
        }
        match opts.open(&lock) {
            Ok(mut f) => {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
                if let Ok(mut g) = LOCK.lock() {
                    *g = Some(f);
                }
            }
            Err(_) => return s,
        }
        s.main = true;
        s.unexpected = existed;
        s.crash = std::fs::read_to_string(dir.join("crash_pending.txt")).ok().filter(|t| !t.trim().is_empty());
        s.recovery = s.next_recovery();
        s
    }

    /// Prochaine sauvegarde retrouvée à proposer (pas celle de cette fenêtre, ni une déjà récupérée).
    fn next_recovery(&self) -> Option<Recovery> {
        if !self.main {
            return None;
        }
        list_autosaves(&data_dir()).into_iter().find(|r| r.slot != self.slot && !self.stale.contains(&r.slot))
    }

    /// Le projet a changé depuis la dernière sauvegarde.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.rev += 1;
    }

    fn clear_mine(&mut self) {
        if self.enabled {
            let dir = data_dir();
            bump_gen_and_clear(&dir, &self.slot);
            for s in self.stale.drain(..) {
                clear_autosave(&dir, &s);
            }
        }
    }

    /// Le projet vient d'être enregistré par la personne.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.clear_mine();
    }

    /// Un projet vient d'être ouvert : rien à enregistrer (la sauvegarde automatique du projet quitté est retirée).
    pub fn mark_loaded(&mut self) {
        self.dirty = false;
        self.clear_mine();
    }

    /// Une sauvegarde retrouvée vient d'être ouverte : effacée dès que le projet récupéré est sauvegardé ailleurs ;
    /// la sauvegarde automatique est faite tout de suite.
    pub fn mark_recovered(&mut self, slot: &str) {
        self.stale.push(slot.to_string());
        self.touch();
        self.next_auto = 0.0;
        self.recovery = self.next_recovery();
    }

    /// Fermeture normale : sauvegarde automatique de cette fenêtre retirée (après l'écriture en cours), les
    /// sauvegardes retrouvées encore en attente sont gardées.
    pub fn clean_exit(&mut self) {
        if self.enabled {
            for _ in 0..50 {
                if !self.busy.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            self.clear_mine();
        }
        release_lock();
    }
}

/// Ferme proprement : le verrou disparaît (donc pas d'« arrêt inattendu » au prochain démarrage).
pub fn release_lock() {
    let taken = LOCK.lock().ok().and_then(|mut g| g.take());
    if taken.is_some() {
        drop(taken);
        let _ = std::fs::remove_file(data_dir().join("running.lock"));
    }
}

/// Demande une sauvegarde automatique tout de suite et attend (4 s au plus) qu'elle soit faite.
pub fn flush_autosave(ctx: Option<&egui::Context>) {
    if !AUTOSAVE_ON.load(Ordering::SeqCst) {
        return; // sauvegarde automatique inactive : rien à faire
    }
    AUTOSAVE_NOW.store(true, Ordering::SeqCst);
    for _ in 0..40 {
        if let Some(c) = ctx {
            c.request_repaint();
        }
        if !AUTOSAVE_NOW.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ------------------------------------------------------------------ dans l'application

impl DwApp {
    /// Compteur de performance (menu Affichage) : images par seconde, dessin, mémoire graphique, annulation.
    pub fn draw_perf(&self, painter: &egui::Painter, rect: egui::Rect) {
        if !self.opts.show_perf {
            return;
        }
        let idle = painter.ctx().input(|i| i.time) - self.last_active > 1.5;
        let mut lines = vec![if idle { "au repos : vue redessinée lentement (bouge la souris pour mesurer)".to_string() } else { format!("{:.0} images/s  ({:.1} ms)", self.fps, 1000.0 / self.fps.max(0.1)) }];
        if let Some(g) = &self.game.gpu {
            if let Ok(g) = g.lock() {
                let (m, t) = g.memory_mb();
                lines.push(format!("{} modèles · {} boîtes · {} k triangles", g.stats.0, g.stats.1, g.stats.2 / 1000));
                let (bm, bt) = g.budget.map(|(a, b)| (a >> 20, b >> 20)).unwrap_or((0, 0));
                lines.push(format!("mémoire graphique : {m} / {bm} Mo de modèles, {t} / {bt} Mo de textures"));
                lines.push(match g.vram_mb {
                    Some(v) => format!("carte graphique : {v} Mo"),
                    None => "carte graphique : mémoire inconnue (1 Go supposé)".to_string(),
                });
            }
        }
        let objs: usize = self.proj.ymaps.iter().filter(|y| !y.sample).map(|y| y.entities.len()).sum();
        lines.push(format!("projet : {objs} objets · annulation : {} pas, ≈ {} Mo", self.undo.len(), undo_bytes(&self.undo, &self.proj) >> 20));
        let font = egui::FontId::monospace(12.0);
        let h = lines.len() as f32 * 16.0 + 10.0;
        let w = 440.0;
        let r = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), egui::vec2(w, h));
        painter.rect_filled(r, 5.0, egui::Color32::from_black_alpha(170));
        for (i, l) in lines.iter().enumerate() {
            let col = if i == 0 && self.fps < 25.0 && !idle { egui::Color32::from_rgb(255, 130, 100) } else { egui::Color32::from_rgb(210, 235, 210) };
            painter.text(r.min + egui::vec2(8.0, 6.0 + i as f32 * 16.0), egui::Align2::LEFT_TOP, l, font.clone(), col);
        }
    }

    /// Chaque image : sauvegarde automatique, panne volontaire de test, demande de fermeture.
    pub fn safety_tick(&mut self, ctx: &egui::Context) {
        if !self.safe.enabled {
            return;
        }
        let now = ctx.input(|i| i.time);
        if let Some(f) = self.safe.test_panic {
            if self.frame >= f {
                panic!("panne volontaire de test (DW_TEST_PANIC)");
            }
        }
        if self.safe.test_touch && self.frame == 30 {
            self.safe.touch();
        }
        // fermeture avec un projet non enregistré
        if ctx.input(|i| i.viewport().close_requested()) && self.safe.dirty && !self.safe.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.safe.confirm_quit = true;
        }
        // résultat de la dernière écriture : en cas d'échec, nouvel essai dans 30 s et erreur montrée
        let done = self.safe.result.lock().ok().and_then(|mut g| g.take());
        match done {
            Some(Ok(rev)) => {
                self.safe.auto_rev = self.safe.auto_rev.max(rev);
                if self.safe.auto_err.take().is_some() {
                    self.status = crate::i18n::t("Sauvegarde automatique rétablie.").into();
                }
                // le projet récupéré est maintenant dans la sauvegarde de cette fenêtre
                let dir = data_dir();
                for s in self.safe.stale.drain(..) {
                    clear_autosave(&dir, &s);
                }
            }
            Some(Err(e)) => {
                let msg = format!("Sauvegarde automatique impossible : {e} (nouvel essai dans 30 s)");
                append(&data_dir().join("crash.log"), &format!("{} UTC — {msg}\n", utc_string(now_unix())));
                self.status = msg;
                self.safe.auto_err = Some(e);
                self.safe.next_auto = now + self.safe.every.min(30.0);
            }
            None => {}
        }
        let forced = AUTOSAVE_NOW.load(Ordering::SeqCst);
        let s = &mut self.safe;
        if !s.dirty || s.rev == s.auto_rev {
            if forced && !s.busy.load(Ordering::SeqCst) {
                AUTOSAVE_NOW.store(false, Ordering::SeqCst);
            }
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        if (!forced && now < s.next_auto) || s.busy.load(Ordering::SeqCst) {
            return;
        }
        s.next_auto = now + s.every;
        s.busy.store(true, Ordering::SeqCst);
        let proj = self.proj.clone();
        let original = self.project_path.as_ref().map(|p| p.display().to_string());
        let busy = s.busy.clone();
        let result = s.result.clone();
        let (slot, rev, gen) = (s.slot.clone(), s.rev, GEN.load(Ordering::SeqCst));
        std::thread::spawn(move || {
            let r = autosave_job(&data_dir(), &slot, gen, proj, original);
            if let Err(e) = &r {
                eprintln!("sauvegarde automatique : {e}");
            }
            if let Ok(mut g) = result.lock() {
                *g = Some(r.map(|_| rev));
            }
            busy.store(false, Ordering::SeqCst);
            if forced {
                AUTOSAVE_NOW.store(false, Ordering::SeqCst);
            }
        });
    }

    /// Modification par un curseur, une valeur glissée ou une case : un seul pas d'annulation par geste (au premier
    /// changement d'un appui de souris), ensuite le projet est seulement marqué modifié. À appeler avant d'appliquer
    /// la nouvelle valeur au projet.
    pub fn undo_once(&mut self, ctx: &egui::Context) {
        let press = ctx.input(|i| if i.pointer.any_down() { i.pointer.press_start_time() } else { None });
        if press.is_some() && press == self.undo_gesture {
            self.safe.touch();
        } else {
            self.push_undo();
        }
        self.undo_gesture = press;
    }

    /// Ouvre une sauvegarde retrouvée (une copie : le projet n'a pas de fichier).
    pub fn open_recovery(&mut self, rec: &Recovery) {
        match read_autosave(&data_dir(), &rec.slot) {
            Some(src) if self.open_project_str(&src, Path::new("")) => {
                self.project_path = None;
                self.safe.mark_recovered(&rec.slot);
                self.status = format!("Projet « {} » récupéré : enregistrez-le (Ctrl+S).", rec.name);
            }
            Some(_) => {
                // illisible : gardée sur le disque, on passe à la suivante
                self.status = format!("{} {}", crate::i18n::t("La sauvegarde automatique est illisible."), self.status);
                let skip = rec.slot.clone();
                let s = &self.safe;
                self.safe.recovery = if s.main { list_autosaves(&data_dir()).into_iter().find(|r| r.slot != s.slot && r.slot != skip && !s.stale.contains(&r.slot)) } else { None };
            }
            None => {
                self.status = crate::i18n::t("La sauvegarde automatique est illisible.").into();
                self.safe.recovery = self.safe.next_recovery();
            }
        }
    }

    /// Ouvrir un projet alors que le projet actuel a des modifications non enregistrées : Enregistrer / Ne pas
    /// enregistrer / Annuler, comme à la fermeture de la fenêtre.
    fn pending_open_ui(&mut self, ctx: &egui::Context) {
        let Some(pend) = self.safe.pending_open.clone() else { return };
        let what = match &pend {
            PendingOpen::File(p) => p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
            PendingOpen::Recovery(r) => format!("{} (sauvegarde automatique)", r.name),
        };
        let mut act = 0;
        egui::Window::new("Ouvrir un autre projet").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("« {} » a des modifications non enregistrées. Les enregistrer avant d'ouvrir « {what} » ?", self.proj.name));
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("Enregistrer")).clicked() {
                    act = 1;
                }
                if ui.button(crate::i18n::t("Ne pas enregistrer")).clicked() {
                    act = 2;
                }
                if ui.button(crate::i18n::t("Annuler")).clicked() {
                    act = 3;
                }
            });
        });
        if act == 1 {
            self.save_project(false);
            if self.safe.dirty {
                return; // enregistrement annulé ou raté : la question reste
            }
        }
        if act == 1 || act == 2 {
            self.safe.pending_open = None;
            match pend {
                PendingOpen::File(p) => self.open_project_file(&p),
                PendingOpen::Recovery(r) => self.open_recovery(&r),
            }
        } else if act == 3 {
            self.safe.pending_open = None;
        }
    }

    /// Fenêtres : rapport d'erreur, projet retrouvé, confirmation de fermeture.
    pub fn safety_ui(&mut self, ctx: &egui::Context) {
        self.pending_open_ui(ctx);
        if !self.safe.enabled {
            return;
        }
        let dir = data_dir();
        // --- session précédente
        if self.safe.crash.is_some() || self.safe.unexpected {
            let mut close = false;
            let report = self.safe.crash.clone();
            egui::Window::new("DragoWalker s'est arrêté la dernière fois").collapsible(false).resizable(true).default_width(520.0).anchor(egui::Align2::CENTER_TOP, [0.0, 90.0]).show(ctx, |ui| {
                match &report {
                    Some(r) => {
                        ui.label(crate::i18n::t("Une erreur interne a été enregistrée. Copiez ce rapport et envoyez-le pour qu'elle soit corrigée (les chemins personnels en ont été retirés)."));
                        egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                            ui.add(egui::TextEdit::multiline(&mut r.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                        });
                    }
                    None => {
                        ui.label(crate::i18n::t("La session précédente ne s'est pas fermée normalement (fermeture forcée, coupure, arrêt brutal). Aucune erreur interne n'a été enregistrée."));
                    }
                }
                ui.horizontal(|ui| {
                    if let Some(r) = &report {
                        if ui.button(crate::i18n::t("Copier le rapport")).clicked() {
                            ctx.copy_text(r.clone());
                        }
                    }
                    if ui.button(crate::i18n::t("Ouvrir le dossier")).clicked() {
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    }
                    ui.hyperlink_to(crate::i18n::t("Signaler sur GitHub"), "https://github.com/xLaugh/DragoWalker/issues/new");
                    if ui.button(crate::i18n::t("Fermer")).clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.safe.crash = None;
                self.safe.unexpected = false;
                let _ = std::fs::remove_file(dir.join("crash_pending.txt"));
            }
        }
        // --- projet retrouvé
        if let Some(rec) = self.safe.recovery.clone() {
            let ago = now_unix().saturating_sub(rec.when) / 60;
            let mut act = if self.safe.test_recover && self.frame == 40 { 1 } else { 0 };
            egui::Window::new("Projet non enregistré retrouvé").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -110.0]).show(ctx, |ui| {
                ui.label(format!("Une sauvegarde automatique de « {} » date d'il y a {ago} min (version {}).", rec.name, rec.version));
                if let Some(o) = &rec.original {
                    ui.label(egui::RichText::new(format!("Projet d'origine : {o}")).small().weak());
                }
                ui.label(crate::i18n::t("Le récupérer ouvre une copie : enregistrez-la sous le nom voulu."));
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("Récupérer")).clicked() {
                        act = 1;
                    }
                    if ui.button(crate::i18n::t("Supprimer")).clicked() {
                        act = 2;
                    }
                });
            });
            if act == 1 {
                if self.safe.dirty {
                    self.safe.pending_open = Some(PendingOpen::Recovery(rec));
                } else {
                    self.open_recovery(&rec);
                }
            } else if act == 2 {
                clear_autosave(&dir, &rec.slot);
                self.safe.recovery = self.safe.next_recovery();
            }
        }
        // --- fermeture
        if self.safe.confirm_quit {
            let mut act = 0;
            egui::Window::new("Projet non enregistré").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(format!("« {} » a des modifications non enregistrées.", self.proj.name));
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("Enregistrer et quitter")).clicked() {
                        act = 1;
                    }
                    if ui.button(crate::i18n::t("Quitter sans enregistrer")).clicked() {
                        act = 2;
                    }
                    if ui.button(crate::i18n::t("Annuler")).clicked() {
                        act = 3;
                    }
                });
            });
            match act {
                1 => {
                    self.save_project(false);
                    if !self.safe.dirty {
                        self.safe.confirm_quit = false;
                        self.safe.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                2 => {
                    self.safe.confirm_quit = false;
                    self.safe.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                3 => self.safe.confirm_quit = false,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;
    use crate::model::{Entity, Ymap};

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dw_safety_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn project_with(n: usize) -> Project {
        let mut y = Ymap::new("map1");
        for i in 0..n {
            let e = Entity::new("prop_bench_01a", v3(i as f32, 0.0, 0.0));
            y.entities.push(e);
        }
        Project { name: "essai".into(), ymaps: vec![y], ..Default::default() }
    }

    #[test]
    fn date_utc() {
        assert_eq!(utc_string(0), "1970-01-01 00:00:00");
        assert_eq!(utc_string(1_000_000_000), "2001-09-09 01:46:40");
        assert_eq!(utc_string(1_795_000_000), "2026-11-18 11:06:40");
    }

    #[test]
    fn sauvegarde_automatique_aller_retour() {
        let d = tmpdir("auto");
        assert!(read_autosave_info(&d, "autosave-1").is_none());
        write_autosave(&d, "autosave-1", project_with(50), Some("C:\\x\\a.dwproj".into())).unwrap();
        let info = read_autosave_info(&d, "autosave-1").expect("infos");
        assert_eq!(info.name, "essai");
        assert_eq!(info.slot, "autosave-1");
        assert_eq!(info.original.as_deref(), Some("C:\\x\\a.dwproj"));
        let p: Project = serde_json::from_str(&read_autosave(&d, "autosave-1").unwrap()).unwrap();
        assert_eq!(p.ymaps.iter().map(|y| y.entities.len()).sum::<usize>(), 50);
        // sans les infos (écriture interrompue), la sauvegarde est ignorée
        std::fs::remove_file(autosave_meta(&d, "autosave-1")).unwrap();
        assert!(read_autosave_info(&d, "autosave-1").is_none());
        write_autosave(&d, "autosave-1", project_with(1), None).unwrap();
        clear_autosave(&d, "autosave-1");
        assert!(read_autosave_info(&d, "autosave-1").is_none() && read_autosave(&d, "autosave-1").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn emplacements_separes() {
        let d = tmpdir("slots");
        // ancien nom (versions précédentes) et deux fenêtres : aucune n'écrase l'autre
        write_autosave(&d, "autosave", project_with(1), None).unwrap();
        write_autosave(&d, "autosave-10-5", project_with(2), None).unwrap();
        write_autosave(&d, "autosave-11-5", project_with(3), None).unwrap();
        std::fs::write(d.join("autre.json"), b"{}").unwrap();
        let l = list_autosaves(&d);
        assert_eq!(l.len(), 3);
        let n = |slot: &str| serde_json::from_str::<Project>(&read_autosave(&d, slot).unwrap()).unwrap().ymaps[0].entities.len();
        assert_eq!((n("autosave"), n("autosave-10-5"), n("autosave-11-5")), (1, 2, 3));
        clear_autosave(&d, "autosave-10-5");
        assert_eq!(list_autosaves(&d).len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ecriture_atomique() {
        let d = tmpdir("atomic");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("a.json");
        write_atomic(&f, b"un").unwrap();
        write_atomic(&f, b"deux").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"deux");
        assert!(!tmp_path(&f).exists());
        // échec (dossier absent) : pas de fichier temporaire laissé, erreur rendue
        let g = d.join("absent").join("b.json");
        assert!(write_atomic(&g, b"x").is_err());
        assert!(!tmp_path(&g).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn enregistrer_pendant_une_sauvegarde_ne_la_ressuscite_pas() {
        let d = tmpdir("gen");
        let g = GEN.load(Ordering::SeqCst);
        // le projet est enregistré (génération changée) pendant que le fil écrivait : sa sauvegarde est retirée
        bump_gen_and_clear(&d, "autosave-9-9");
        assert!(autosave_job(&d, "autosave-9-9", g, project_with(1), None).is_ok());
        assert!(read_autosave_info(&d, "autosave-9-9").is_none());
        // sans enregistrement entre-temps, elle reste
        let g = GEN.load(Ordering::SeqCst);
        autosave_job(&d, "autosave-9-9", g, project_with(1), None).unwrap();
        assert!(read_autosave_info(&d, "autosave-9-9").is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn historique_borne_et_tableaux_partages() {
        let mut cur = project_with(10);
        cur.terrain = Some(crate::terrain::Terrain::new(v3(0.0, 0.0, 0.0), 800.0, 5.0, 1));
        let big = cur.terrain.as_ref().unwrap().h.len() * 4;
        let mut undo: Vec<Project> = Vec::new();
        // 30 pas qui ne touchent pas au terrain : il n'est compté qu'une fois
        for _ in 0..30 {
            undo.push(cur.clone());
        }
        assert!(undo_bytes(&undo, &cur) < light_bytes(&cur) * 30 + big / 2, "terrain partagé avec le projet actuel : pas recompté");
        // 30 pas qui modifient le terrain : chacun a son tableau
        let mut own = cur.clone();
        let mut undo2: Vec<Project> = Vec::new();
        for i in 0..30 {
            undo2.push(own.clone());
            let t = own.terrain.as_mut().unwrap();
            std::sync::Arc::make_mut(&mut t.h)[0] = i as f32;
        }
        assert!(undo_bytes(&undo2, &own) >= light_bytes(&own) * 30 + 29 * big);
        // beaucoup de pas : bornés à 100
        let mut many: Vec<Project> = (0..130).map(|_| project_with(1)).collect();
        trim_undo(&mut many, &project_with(1));
        assert_eq!(many.len(), UNDO_MAX_STEPS);
    }

    #[test]
    fn annuler_un_coup_de_pinceau_restaure_le_terrain() {
        let mut cur = project_with(3);
        cur.terrain = Some(crate::terrain::Terrain::new(v3(0.0, 0.0, 0.0), 200.0, 5.0, 1));
        let avant = cur.terrain.as_ref().unwrap().h.to_vec();
        let mat_avant = cur.terrain.as_ref().unwrap().mat.to_vec();
        let snapshot = cur.clone(); // ce que garde push_undo
        let t = cur.terrain.as_mut().unwrap();
        t.brush(crate::terrain::Brush::Raise, v3(0.0, 0.0, 5.0), 30.0, 5.0, 1.0, 1, 0.0);
        t.brush(crate::terrain::Brush::Paint, v3(0.0, 0.0, 5.0), 30.0, 5.0, 1.0, 3, 0.0);
        assert_ne!(t.h.to_vec(), avant, "le pinceau a bien modifié le terrain actuel");
        assert_ne!(t.mat.to_vec(), mat_avant);
        // la copie d'annulation n'a pas bougé
        let s = snapshot.terrain.as_ref().unwrap();
        assert_eq!(s.h.to_vec(), avant);
        assert_eq!(s.mat.to_vec(), mat_avant);
        // annuler = remettre la copie
        let mut courant = cur;
        courant = std::mem::replace(&mut courant, snapshot);
        assert_eq!(courant.terrain.as_ref().unwrap().h.len(), avant.len());
    }

    #[test]
    fn historique_borne_par_la_memoire() {
        // 50 pas de 40 000 objets (≈ 15 Mo chacun d'après l'estimation) : bien au-dessus de 600 Mo
        let cur = project_with(1);
        let mut undo: Vec<Project> = (0..50).map(|_| project_with(40_000)).collect();
        let before = undo.len();
        trim_undo(&mut undo, &cur);
        assert!(undo.len() < before && undo.len() >= UNDO_MIN_STEPS);
        assert!(undo_bytes(&undo, &cur) <= UNDO_BUDGET || undo.len() == UNDO_MIN_STEPS);
    }

    #[test]
    fn rapport_d_erreur() {
        let r = format_report("0.2.4", 1_795_000_000, "main", "boum", "src\\terrain.rs:10:5");
        assert!(r.contains("DragoWalker 0.2.4") && r.contains("2026-11-18 11:06:40 UTC") && r.contains("boum") && r.contains("terrain.rs:10"));
    }

    /// Mesure à la main : cargo test --release mesure_copie -- --ignored --nocapture
    #[test]
    #[ignore]
    fn mesure_copie_projet() {
        for n in [1_000usize, 10_000, 50_000, 200_000] {
            let p = project_with(n);
            let t0 = std::time::Instant::now();
            let c = p.clone();
            let dt = t0.elapsed();
            println!("{n:>7} objets : copie {:>7.2} ms (estimation {} Mo)", dt.as_secs_f64() * 1000.0, light_bytes(&c) >> 20);
            std::mem::forget(c);
        }
    }
}
