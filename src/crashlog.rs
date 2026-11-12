//! Analyse d'un plantage de FiveM : lit le journal (CitizenFX_log_….log) et crashometry.json que FiveM garde
//! à chaque plantage, et explique ce qui s'est passé juste avant — réserve (pool) pleine, fichiers ignorés dans
//! stream/, erreurs de scripts, ressources lancées en dernier… Le journal du jeu en direct est aussi lisible ici.
//!
//! L'analyse dit ce que le journal montre ; elle ne devine pas la cause d'une adresse de plantage
//! (GTA5_b3889.exe+E93DB6) : seul le .dmp, lu avec un débogueur, le permet.

use crate::app::DwApp;
use crate::check::Level;
use eframe::egui::{self, Color32, RichText};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Finding {
    pub level: Level,
    pub title: String,
    pub detail: String,
}

#[derive(Default)]
pub struct Analysis {
    pub source: String,
    pub findings: Vec<Finding>,
    pub facts: Vec<(String, String)>,
    /// derniers événements avant le plantage : (secondes avant, message)
    pub before: Vec<(f64, String)>,
}

fn clean(s: &str) -> String {
    // codes de couleur de FiveM (^1 … ^9)
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '^' && it.peek().map(|n| n.is_ascii_digit()).unwrap_or(false) {
            it.next();
            continue;
        }
        out.push(c);
    }
    out
}

/// "[     95375] [  Thread]   pid/ message" -> (95375, "message")
fn split_line(l: &str) -> Option<(u64, String)> {
    let rest = l.strip_prefix('[')?;
    let end = rest.find(']')?;
    let ms: u64 = rest[..end].trim().parse().ok()?;
    let msg = match l.find("/ ") {
        Some(p) => &l[p + 2..],
        None => &rest[end + 1..],
    };
    Some((ms, clean(msg.trim_end())))
}

fn is_noise(m: &str) -> bool {
    ["RunInitFunctions", "Still executing", "Registering EventHandler", "Instantiated instance", "Returning device", "perfectly normal", "Loaded dw_", "Loaded ", "Dropping duplicate"].iter().any(|n| m.contains(n))
}

fn is_frame(m: &str) -> bool {
    let t = m.trim();
    let Some((module, off)) = t.split_once('+') else { return false };
    (module.ends_with(".exe") || module.ends_with(".dll")) && !module.contains(' ') && !off.is_empty() && off.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_problem(m: &str) -> bool {
    let l = m.to_lowercase();
    if l.contains("perfectly normal") || l.contains("present errors") {
        return false;
    }
    ["script error", "failed to", "couldn't ", "could not", "error:", "error loading", "exception", "attempt to", "no such export", "unable to"].iter().any(|k| l.contains(k))
}

/// Analyse le texte d'un journal FiveM (et, si on l'a, le crashometry.json).
pub fn analyse(log: &str, crashometry: Option<&str>) -> Analysis {
    let mut a = Analysis::default();
    let lines: Vec<(u64, String)> = log.lines().filter_map(split_line).collect();
    if lines.is_empty() {
        a.findings.push(Finding { level: Level::Warning, title: "Journal illisible ou vide".into(), detail: "Ce fichier ne ressemble pas à un journal FiveM (CitizenFX_log_….log).".into() });
        return a;
    }
    let crash_idx = lines.iter().position(|(_, m)| m.contains("Process crash captured"));
    let end_ms = crash_idx.map(|i| lines[i].0).unwrap_or(lines.last().map(|l| l.0).unwrap_or(0));
    let upto = crash_idx.unwrap_or(lines.len());

    // --- le plantage lui-même
    let mut pool: Option<(String, String)> = None;
    if let Some(ci) = crash_idx {
        let mut dialog: Vec<String> = Vec::new();
        let mut frames: Vec<String> = Vec::new();
        let mut in_stack = false;
        for (_, m) in &lines[ci + 1..] {
            let t = m.trim();
            if t.is_empty() {
                continue;
            }
            if t.starts_with("Stack trace") {
                in_stack = true;
                continue;
            }
            if in_stack {
                if is_frame(t) {
                    frames.push(t.to_string());
                }
            } else {
                dialog.push(t.to_string());
            }
        }
        let first = dialog.first().cloned().unwrap_or_default();
        if let Some(p) = dialog.iter().chain(lines[..ci].iter().map(|l| &l.1)).find(|m| m.contains("Pool Full")) {
            // « <<unknown pool>> Pool Full, Size == 256 » ou « Error: Peds Pool Full, Size == 256 »
            let t = p.trim_start_matches("Error:").trim();
            let name = t.split(" Pool Full").next().unwrap_or("").trim().to_string();
            let size = t.split("Size ==").nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
            pool = Some((name, size));
        }
        a.facts.push(("Plantage".into(), format!("{:.1} s après le démarrage du jeu", end_ms as f64 / 1000.0)));
        if let Some(m) = lines.iter().find(|l| l.1.contains("Game finished loading")) {
            a.facts.push(("Après le chargement".into(), format!("{:.1} s entre la fin du chargement et le plantage", end_ms.saturating_sub(m.0) as f64 / 1000.0)));
        }
        if !first.is_empty() {
            a.facts.push(("Message du jeu".into(), first.clone()));
        }
        if let Some(h) = dialog.iter().find(|d| d.to_lowercase().contains("crash hash")) {
            a.facts.push(("Code".into(), h.clone()));
        }
        if !frames.is_empty() {
            a.facts.push(("Pile d'appels".into(), frames.iter().take(4).cloned().collect::<Vec<_>>().join("  →  ")));
        }
        let engine = frames.first().map(|f| f.to_lowercase().starts_with("gta5")).unwrap_or(false);
        let title = match (&pool, engine) {
            (Some(_), _) => "Le jeu a planté : une de ses réserves (pool) est pleine".to_string(),
            (None, true) => "Le jeu a planté dans son propre code (GTA5.exe)".to_string(),
            (None, false) => "Le jeu a planté".to_string(),
        };
        let detail = match &pool {
            Some(_) => String::new(),
            None if engine => format!("Adresse {} : c'est dans le moteur de GTA V, appelé par FiveM. Le journal ne dit pas pourquoi ; seule la lecture du fichier .dmp avec un débogueur le permet. Cherche plutôt ce qui venait de se charger (liste « Juste avant »).", frames.first().cloned().unwrap_or_default()),
            None => format!("Premier module de la pile : {}. Si ce n'est pas GTA5.exe, le pilote graphique ou un module externe peut être en cause.", frames.first().cloned().unwrap_or_else(|| "inconnu".into())),
        };
        a.findings.push(Finding { level: Level::Error, title, detail });
    } else {
        a.findings.push(Finding { level: Level::Ok, title: "Aucun plantage dans ce journal".into(), detail: "Le jeu a été fermé sans plantage enregistré (ou ce n'est pas le bon journal).".into() });
    }

    // --- réserve pleine
    if let Some((name, size)) = &pool {
        let known = name.is_empty() || name.contains("unknown");
        let mut d = format!("Réserve « {} », {} places.", if known { "nom inconnu" } else { name.as_str() }, if size.is_empty() { "?".into() } else { size.clone() });
        if size == "256" || size.is_empty() {
            d += " Dans GTA V, seules quelques réserves ont 256 places : les PNJ et leurs activités (Peds, CPropManagementHelper, CTaskUseScenarioEntityExtension) et les recherches de trajet des voitures (CPathNodeRouteSearchHelper). Essaie d'abord sans les PNJ (dw_peds.lua), puis sans les voitures de PNJ (dw_traffic.lua), avec « Contenu du test » du serveur de test.";
        } else {
            d += " Réduis le nombre d'objets du type concerné (menu Fichier » Contrôle avant l'export compare ton projet aux réserves du jeu).";
        }
        a.findings.push(Finding { level: Level::Error, title: format!("Réserve (pool) pleine : {}", if known { "nom inconnu".to_string() } else { name.clone() }), detail: d });
    }

    // --- fichiers ignorés dans stream/
    let mut ignored: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, m) in &lines[..upto] {
        if let Some(rest) = m.strip_prefix("can't register ") {
            if let Some((res, tail)) = rest.split_once(":/") {
                let file = tail.split(':').next().unwrap_or("").to_string();
                // « compcache » est le cache interne de FiveM : la vraie ressource est le premier dossier
                match (res, file.split_once('/')) {
                    ("compcache", Some((r, f))) => ignored.entry(r.to_string()).or_default().push(f.to_string()),
                    _ => ignored.entry(res.to_string()).or_default().push(file),
                }
            }
        }
    }
    for (res, files) in &ignored {
        let ex: Vec<&str> = files.iter().take(3).map(|s| s.as_str()).collect();
        let xml = files.iter().filter(|f| f.ends_with(".xml")).count();
        let mut d = format!("{} fichier(s) de la ressource « {res} » ne sont pas reconnus par FiveM (exemples : {}).", files.len(), ex.join(", "));
        if xml > 0 {
            d += " Des fichiers .xml (comme .yft.xml) ne sont pas chargés : il faut les convertir en fichiers binaires (CodeWalker / Sollumz).";
        }
        d += " Un .zip dans stream/ doit être décompressé. Ces fichiers sont ignorés : ce n'est pas la cause directe d'un plantage.";
        a.findings.push(Finding { level: Level::Warning, title: format!("Fichiers ignorés dans « {res} »"), detail: d });
    }

    // --- doublons de modèles
    let dups: Vec<&String> = lines[..upto].iter().filter(|l| l.1.starts_with("Duplicate Archetype")).map(|l| &l.1).collect();
    if !dups.is_empty() {
        a.findings.push(Finding { level: Level::Warning, title: format!("{} modèle(s) déclaré(s) en double", dups.len()), detail: format!("Exemple : {}. Deux fichiers .ytyp définissent le même modèle : sans gravité si l'un vient du jeu, à corriger si les deux viennent de tes ressources.", dups[0]) });
    }

    // --- erreurs de scripts et autres problèmes
    let mut seen: Vec<String> = Vec::new();
    for (_, m) in &lines[..upto] {
        if is_problem(m) && !m.starts_with("can't register") && !m.contains("Pool Full") && !seen.iter().any(|s| s == m) {
            seen.push(m.clone());
        }
    }
    if !seen.is_empty() {
        let more = seen.len().saturating_sub(8);
        let mut d = seen.iter().take(8).cloned().collect::<Vec<_>>().join("\n");
        if more > 0 {
            d += &format!("\n… et {more} autre(s)");
        }
        a.findings.push(Finding { level: Level::Warning, title: format!("{} message(s) d'erreur dans le journal", seen.len()), detail: d });
    }

    // --- ressources et collisions
    let required = lines.iter().find_map(|l| l.1.split("Required resources:").nth(1).map(|s| s.trim().to_string()));
    if let Some(r) = required {
        a.facts.push(("Ressources du serveur".into(), r));
    }
    let mut scripts: Vec<String> = Vec::new();
    for (_, m) in &lines[..upto] {
        if let Some(n) = m.strip_prefix("Creating script environments for ") {
            scripts.push(n.trim().to_string());
        }
    }
    if !scripts.is_empty() {
        let last: Vec<String> = scripts.iter().rev().take(3).rev().cloned().collect();
        a.facts.push(("Derniers scripts lancés".into(), last.join(", ")));
    }
    let dw_col = lines[..upto].iter().filter(|l| l.1.starts_with("Loaded dw_") && l.1.contains(".ybn")).count();
    if dw_col > 0 {
        a.facts.push(("Collisions de DragoWalker chargées".into(), dw_col.to_string()));
    }

    // --- juste avant
    let mut before: Vec<(f64, String)> = lines[..upto].iter().rev().filter(|l| !is_noise(&l.1) && !l.1.trim().is_empty()).take(12).map(|l| ((end_ms.saturating_sub(l.0)) as f64 / 1000.0, l.1.clone())).collect();
    before.reverse();
    a.before = before;

    // --- crashometry.json
    if let Some(c) = crashometry {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(c) {
            let g = |k: &str| v[k].as_str().map(str::to_string);
            for (k, label) in [("crashometry_gpu_name", "Carte graphique"), ("crashometry_last_server", "Serveur"), ("crashometry_last_server_ver", "Version du serveur"), ("crashometry_mod_package_count", "Paquets de mods"), ("crash_hash_key", "Nom du plantage")] {
                if let Some(x) = g(k) {
                    a.facts.push((label.into(), x));
                }
            }
            if g("crashometry_onesync_enabled").as_deref() == Some("true") {
                a.facts.push(("OneSync".into(), "activé".into()));
            }
            if let Some(p) = g("crashometry_pool_error") {
                if pool.is_none() {
                    a.findings.push(Finding { level: Level::Error, title: format!("Réserve (pool) pleine : {p}"), detail: "Indiquée par crashometry.json.".into() });
                }
            }
        }
    }
    a
}

/// Fichiers d'un dossier de plantage : (journal le plus récent, crashometry.json).
fn files_in(dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut crashometry = None;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n.starts_with("citizenfx_log") && n.ends_with(".log") {
                logs.push((e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()));
            } else if n == "crashometry.json" {
                crashometry = Some(e.path());
            }
        }
    }
    logs.sort();
    (logs.pop().map(|l| l.1), crashometry)
}

pub fn analyse_folder(dir: &Path) -> Result<Analysis, String> {
    let (log, cm) = files_in(dir);
    let log = log.ok_or_else(|| format!("Aucun journal CitizenFX_log_….log dans {}", dir.display()))?;
    let text = std::fs::read(&log).map_err(|e| e.to_string())?;
    let cm_text = cm.and_then(|p| std::fs::read_to_string(p).ok());
    let mut a = analyse(&String::from_utf8_lossy(&text), cm_text.as_deref());
    a.source = log.display().to_string();
    Ok(a)
}

/// Dossier FiveM.app : réglage « fivem_dir » (FiveM.app ou son parent), sinon %LOCALAPPDATA%\FiveM\FiveM.app.
pub fn fivem_dir() -> Option<PathBuf> {
    let s = crate::app::load_settings();
    let cand = s["fivem_dir"].as_str().map(PathBuf::from).or_else(|| std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("FiveM")))?;
    [cand.clone(), cand.join("FiveM.app")].into_iter().find(|p| p.join("logs").is_dir())
}

/// Journal le plus récent de FiveM.
pub fn latest_log() -> Option<PathBuf> {
    let dir = fivem_dir()?.join("logs");
    files_in(&dir).0
}

/// Journal de FiveM modifié depuis `since` qui contient un plantage (pour prévenir pendant un test en jeu).
pub fn crashed_since(since: std::time::SystemTime) -> Option<PathBuf> {
    use std::io::{Read, Seek, SeekFrom};
    let path = latest_log()?;
    if std::fs::metadata(&path).and_then(|m| m.modified()).ok()? < since {
        return None;
    }
    let mut f = std::fs::File::open(&path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut buf = Vec::new();
    f.seek(SeekFrom::Start(len.saturating_sub(300_000))).ok()?;
    f.read_to_end(&mut buf).ok()?;
    String::from_utf8_lossy(&buf).contains("Process crash captured").then_some(path)
}

/// Dernières lignes intéressantes (erreurs, fichiers ignorés…) du journal d'un jeu en cours.
pub fn live_problems(path: &Path, max: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(600_000);
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut out: Vec<String> = text
        .lines()
        .filter_map(split_line)
        .filter(|(_, m)| (is_problem(m) || m.contains("Pool Full") || m.contains("crash") || m.starts_with("can't register")) && !m.contains("Loaded "))
        .map(|(ms, m)| format!("[{:>6.1} s] {}", ms as f64 / 1000.0, m))
        .collect();
    let n = out.len().saturating_sub(max);
    out.drain(..n);
    out
}

pub struct CrashState {
    pub open: bool,
    pub analysis: Option<Analysis>,
    pub error: String,
    pub live: Vec<String>,
    pub live_at: f64,
}

impl CrashState {
    pub fn new() -> CrashState {
        let mut s = CrashState { open: false, analysis: None, error: String::new(), live: Vec::new(), live_at: -100.0 };
        // test : DW_TEST_CRASH=<dossier de plantage> ouvre l'analyse tout de suite
        if let Ok(d) = std::env::var("DW_TEST_CRASH") {
            s.open = true;
            match analyse_folder(Path::new(&d)) {
                Ok(a) => s.analysis = Some(a),
                Err(e) => s.error = e,
            }
        }
        s
    }
}

pub fn summary(a: &Analysis) -> String {
    let mut s = format!("Analyse du plantage ({})\n", a.source);
    for f in &a.findings {
        s += &format!("\n[{}] {}\n{}\n", match f.level { Level::Error => "ERREUR", Level::Warning => "ALERTE", Level::Ok => "OK" }, f.title, f.detail);
    }
    s += "\n";
    for (k, v) in &a.facts {
        s += &format!("{k} : {v}\n");
    }
    if !a.before.is_empty() {
        s += "\nJuste avant :\n";
        for (t, m) in &a.before {
            s += &format!("  −{t:.1} s  {m}\n");
        }
    }
    s
}

impl DwApp {
    pub fn crash_window(&mut self, ctx: &egui::Context) {
        if !self.crash.open {
            return;
        }
        let mut open = true;
        let mut act = 0;
        let now = ctx.input(|i| i.time);
        self.show_panel(ctx, "crash", "Analyse de plantage (FiveM)", crate::docking::Dock::Floating, [640.0, 600.0], &mut open, |s, ui| {
            ui.label(RichText::new("Explique le plantage d'un jeu FiveM d'après son journal. Choisis le dossier que FiveM propose après un plantage (celui qui contient CitizenFX_log_….log), ou laisse DragoWalker prendre le dernier journal.").small());
            ui.horizontal_wrapped(|ui| {
                if ui.button("Dernier plantage de FiveM").on_hover_text("Journal le plus récent dans le dossier FiveM.app\\logs").clicked() {
                    act = 1;
                }
                if ui.button("Choisir un dossier…").clicked() {
                    act = 2;
                }
                if ui.button("Dossier de FiveM…").on_hover_text("Si FiveM n'est pas dans %LOCALAPPDATA%\\FiveM : le dossier FiveM.app (ou son parent)").clicked() {
                    act = 3;
                }
                if s.crash.analysis.is_some() && ui.button("Copier le résumé").clicked() {
                    act = 4;
                }
            });
            ui.label(RichText::new(format!("FiveM : {}", crate::crashlog::fivem_dir().map(|p| p.display().to_string()).unwrap_or_else(|| "introuvable (utilise « Dossier de FiveM… »)".into()))).small().weak());
            if !s.crash.error.is_empty() {
                ui.colored_label(Color32::from_rgb(255, 120, 100), &s.crash.error);
            }
            egui::ScrollArea::vertical().id_salt("crash_res").max_height(380.0).show(ui, |ui| {
                if let Some(a) = &s.crash.analysis {
                    ui.label(RichText::new(&a.source).small().weak());
                    for f in &a.findings {
                        let (mark, col) = match f.level {
                            Level::Error => ("✖", Color32::from_rgb(255, 100, 90)),
                            Level::Warning => ("⚠", Color32::from_rgb(255, 200, 80)),
                            Level::Ok => ("✔", Color32::from_rgb(120, 220, 120)),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(mark).color(col));
                            ui.label(RichText::new(&f.title).color(col).strong());
                        });
                        if !f.detail.is_empty() {
                            ui.label(RichText::new(&f.detail).small());
                        }
                        ui.add_space(4.0);
                    }
                    if !a.facts.is_empty() {
                        ui.separator();
                        egui::Grid::new("crash_facts").num_columns(2).striped(true).show(ui, |ui| {
                            for (k, v) in &a.facts {
                                ui.label(RichText::new(k).small().weak());
                                ui.label(RichText::new(v).small());
                                ui.end_row();
                            }
                        });
                    }
                    if !a.before.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Juste avant le plantage").strong());
                        for (t, m) in &a.before {
                            ui.label(RichText::new(format!("−{t:>5.1} s  {m}")).monospace().size(11.0));
                        }
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new("Journal de FiveM en direct (erreurs seulement)").strong());
            egui::ScrollArea::vertical().id_salt("crash_live").max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
                if s.crash.live.is_empty() {
                    ui.label(RichText::new("Rien à signaler (ou FiveM n'a pas encore écrit de journal).").small().weak());
                }
                for l in &s.crash.live {
                    ui.label(RichText::new(l).monospace().size(11.0));
                }
            });
        });
        if now - self.crash.live_at > 3.0 {
            self.crash.live_at = now;
            self.crash.live = latest_log().map(|p| live_problems(&p, 40)).unwrap_or_default();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        self.crash.open = open;
        match act {
            1 => match latest_log() {
                Some(p) => match analyse_folder(p.parent().unwrap_or(Path::new("."))) {
                    Ok(a) => {
                        self.crash.error.clear();
                        self.crash.analysis = Some(a);
                    }
                    Err(e) => self.crash.error = e,
                },
                None => self.crash.error = "Aucun journal FiveM trouvé : indique le dossier de FiveM.".into(),
            },
            2 => {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier du plantage (avec CitizenFX_log_….log)").pick_folder() {
                    match analyse_folder(&d) {
                        Ok(a) => {
                            self.crash.error.clear();
                            self.crash.analysis = Some(a);
                        }
                        Err(e) => self.crash.error = e,
                    }
                }
            }
            3 => {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier FiveM.app (ou le dossier qui le contient)").pick_folder() {
                    crate::app::save_setting("fivem_dir", serde_json::json!(d.display().to_string()));
                    self.crash.live_at = -100.0;
                }
            }
            4 => {
                if let Some(a) = &self.crash.analysis {
                    ctx.copy_text(summary(a));
                    self.status = "Résumé de l'analyse copié.".into();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POOL: &str = "[     86281] [b3889_GTAProce]             MainThrd/ Loaded dw_terrain_col_r2c2.ybn (id 14705)
[     86375] [b3889_GTAProce]             MainThrd/ ^2Game finished loading!
[     87062] [b3889_GTAProce]             MainThrd/ Creating script environments for dw_tools
[     87078] [b3889_GTAProce]             MainThrd/ Creating script environments for nouveau_projet
[     88516] [b3889_GTAProce]             MainThrd/ [DragoWalker] DragoWalker : F2 noclip
[     88953] [b3889_GTAProce]             MainThrd/ Error: <<unknown pool>> Pool Full, Size == 256
[     90453] [    DumpServer]                 8900/ Process crash captured. Crash dialog content:
[     90453] [    DumpServer]                 8900/ <<unknown pool>> Pool Full, Size == 256
[     90453] [    DumpServer]                 8900/
";

    const ENGINE: &str = "[     68516] [b3889_GTAProce]             MainThrd/ can't register faux_pack3:/prop_streetlight_01.yft.xml: no streaming module (does this file even belong in stream?)
[     68516] [b3889_GTAProce]             MainThrd/ can't register faux_pack3:/prop_traffic_01a.yft.xml: no streaming module (does this file even belong in stream?)
[     75250] [b3889_GTAProce]             MainThrd/ Duplicate Archetype 'p_w_ar_musket_chrg' (3E15FEA3), seen in 'Extra' and 'v_minigame.ytyp'
[     86375] [b3889_GTAProce]             MainThrd/ ^2Game finished loading!
[     91609] [b3889_GTAProce]             MainThrd/ Scaleform minimap loaded successfully
[     95375] [    DumpServer]                27948/ Process crash captured. Crash dialog content:
[     95375] [    DumpServer]                27948/ GTA5_b3889.exe+E93DB6
[     95375] [    DumpServer]                27948/ An error at GTA5_b3889.exe+E93DB6 caused FiveM to stop working.
[     95375] [    DumpServer]                27948/ Legacy crash hash: fanta-fourteen-orange
[     95375] [    DumpServer]                27948/ Stack trace:
[     95375] [    DumpServer]                27948/   GTA5_b3889.exe+E93DB6
[     95375] [    DumpServer]                27948/   gta-core-five.dll+89B0F
";

    #[test]
    fn reserve_pleine() {
        let a = analyse(POOL, Some("{\"crashometry_gpu_name\":\"NVIDIA RTX\",\"crashometry_onesync_enabled\":\"true\"}"));
        assert!(a.findings.iter().any(|f| f.level == Level::Error && f.title.contains("pool") && f.title.contains("pleine")));
        let f = a.findings.iter().find(|f| f.detail.contains("256 places")).expect("détail 256");
        assert!(f.detail.contains("dw_peds.lua") && f.detail.contains("CPathNodeRouteSearchHelper"));
        assert!(a.facts.iter().any(|(k, v)| k == "Derniers scripts lancés" && v == "dw_tools, nouveau_projet"));
        assert!(a.facts.iter().any(|(k, v)| k == "Collisions de DragoWalker chargées" && v == "1"));
        assert!(a.facts.iter().any(|(k, v)| k == "Carte graphique" && v.contains("RTX")));
        // 4,1 s entre la fin du chargement (86375) et le plantage (90453) : pas plus de 12 lignes « juste avant »
        assert!(a.before.len() <= 12 && a.before.iter().any(|b| b.1.contains("Pool Full")));
    }

    #[test]
    fn plantage_du_moteur_et_fichiers_ignores() {
        let a = analyse(ENGINE, None);
        assert!(a.findings.iter().any(|f| f.title.contains("propre code") && f.detail.contains("E93DB6")));
        let ig = a.findings.iter().find(|f| f.title.contains("faux_pack3")).expect("fichiers ignorés");
        assert!(ig.detail.contains("2 fichier") && ig.detail.contains(".yft.xml"));
        assert!(a.findings.iter().any(|f| f.title.contains("en double")));
        assert!(a.facts.iter().any(|(k, v)| k == "Pile d'appels" && v.contains("gta-core-five.dll+89B0F")));
        // pas de pool dans ce cas
        assert!(!a.findings.iter().any(|f| f.title.contains("pleine")));
    }

    #[test]
    fn journal_sans_plantage_ou_illisible() {
        let a = analyse("[     1000] [x] MainThrd/ tout va bien\n", None);
        assert!(a.findings.iter().any(|f| f.level == Level::Ok));
        let b = analyse("pas un journal", None);
        assert!(b.findings.iter().any(|f| f.level == Level::Warning));
    }
}
