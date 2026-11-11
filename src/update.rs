//! Mises à jour : au démarrage, DragoWalker regarde la dernière release GitHub (xLaugh/DragoWalker).
//! Si elle est plus récente, un bandeau propose de la télécharger :
//! - version installée (unins000.exe à côté de l'exe) : le setup est téléchargé puis lancé en silencieux,
//!   il remplace l'app et la relance ;
//! - version portable : le zip est téléchargé, le nouvel exe remplace celui-ci (l'ancien est renommé
//!   DragoWalker.old.exe, supprimé au lancement suivant) et l'app redémarre.
//! Pas de vérification en build debug ni en mode test (variables DW_*), sauf DW_UPDATE_TEST=1.

use eframe::egui;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const API: &str = "https://api.github.com/repos/xLaugh/DragoWalker/releases/latest";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Version locale ; en test, DW_UPDATE_TEST=auto:0.1.0 fait croire à une ancienne version et installe seul.
fn current() -> String {
    test_mode().map(|(v, _)| v).unwrap_or_else(|| CURRENT.to_string())
}

fn test_mode() -> Option<(String, bool)> {
    let v = std::env::var("DW_UPDATE_TEST").ok()?;
    let auto = v.starts_with("auto:");
    let ver = v.trim_start_matches("auto:");
    if ver.contains('.') { Some((ver.to_string(), auto)) } else { None }
}

#[derive(Clone)]
enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(String, f32),
    Error(String),
    Hidden,
}

#[derive(Clone)]
struct Release {
    version: String,
    page: String,
    setup_url: Option<String>,
    zip_url: Option<String>,
}

fn state() -> &'static Arc<Mutex<State>> {
    static S: OnceLock<Arc<Mutex<State>>> = OnceLock::new();
    S.get_or_init(|| Arc::new(Mutex::new(State::Idle)))
}

fn set(s: State) {
    if let Ok(mut g) = state().lock() {
        *g = s;
    }
}

/// "0.2.10" -> (0, 2, 10) ; le « v » devant est ignoré.
fn parse(v: &str) -> (u32, u32, u32) {
    let mut it = v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map(|x| x.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn is_newer(remote: &str, local: &str) -> bool {
    parse(remote) > parse(local)
}

fn exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// Installée par le setup : le désinstalleur d'Inno Setup est à côté de l'exe.
fn is_installed() -> bool {
    exe_path().and_then(|p| p.parent().map(|d| d.join("unins000.exe").is_file())).unwrap_or(false)
}

/// À appeler une fois au démarrage : nettoie l'ancien exe et lance la vérification en arrière-plan.
pub fn start() {
    if let Some(dir) = exe_path().and_then(|p| p.parent().map(Path::to_path_buf)) {
        let _ = std::fs::remove_file(dir.join("DragoWalker.old.exe"));
    }
    let test = std::env::var_os("DW_UPDATE_TEST").is_some();
    let dev = cfg!(debug_assertions) || std::env::vars().any(|(k, _)| k.starts_with("DW_"));
    if dev && !test {
        set(State::Hidden);
        return;
    }
    check();
}

fn check() {
    set(State::Checking);
    std::thread::spawn(|| match fetch_latest() {
        Ok(r) if is_newer(&r.version, &current()) => {
            if test_mode().map(|(_, a)| a).unwrap_or(false) {
                install(r, None);
            } else {
                set(State::Available(r));
            }
        }
        Ok(_) => set(State::UpToDate),
        // pas de réseau, GitHub indisponible… : on ne dérange pas l'utilisateur
        Err(_) => set(State::Hidden),
    });
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(10)).user_agent(&format!("DragoWalker/{CURRENT}")).build()
}

fn fetch_latest() -> Result<Release, String> {
    let v: serde_json::Value = agent().get(API).set("Accept", "application/vnd.github+json").timeout(std::time::Duration::from_secs(20)).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("pas de version")?.to_string();
    let mut r = Release { version: tag.trim_start_matches(['v', 'V']).to_string(), page: v["html_url"].as_str().unwrap_or("").to_string(), setup_url: None, zip_url: None };
    for a in v["assets"].as_array().into_iter().flatten() {
        let name = a["name"].as_str().unwrap_or("").to_lowercase();
        let url = a["browser_download_url"].as_str().map(str::to_string);
        if name.ends_with("-setup.exe") {
            r.setup_url = url;
        } else if name.ends_with(".zip") {
            r.zip_url = url;
        }
    }
    Ok(r)
}

/// Télécharge `url` en mémoire en mettant à jour la progression.
fn download(url: &str, version: &str) -> Result<Vec<u8>, String> {
    let resp = agent().get(url).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| e.to_string())?;
    let total: usize = resp.header("content-length").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut out = Vec::with_capacity(total.max(1 << 20));
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if total > 0 {
            set(State::Downloading(version.to_string(), out.len() as f32 / total as f32));
        }
    }
    Ok(out)
}

/// Télécharge puis applique la mise à jour ; en cas de succès, l'app se ferme (le nouvel exe prend le relais).
fn install(r: Release, ctx: Option<egui::Context>) {
    std::thread::spawn(move || {
        set(State::Downloading(r.version.clone(), 0.0));
        if let Some(c) = &ctx {
            c.request_repaint();
        }
        let res = if is_installed() { install_setup(&r) } else { install_portable(&r) };
        match res {
            Ok(()) => {
                if let Some(c) = &ctx {
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                // l'exe doit être libéré au plus vite pour que le setup puisse le remplacer
                std::thread::sleep(std::time::Duration::from_millis(1500));
                std::process::exit(0);
            }
            Err(e) => set(State::Error(e)),
        }
        if let Some(c) = &ctx {
            c.request_repaint();
        }
    });
}

fn install_setup(r: &Release) -> Result<(), String> {
    let url = r.setup_url.as_deref().ok_or("cette release n'a pas d'installeur")?;
    let data = download(url, &r.version)?;
    let path = std::env::temp_dir().join(format!("DragoWalker-{}-setup.exe", r.version));
    std::fs::write(&path, data).map_err(|e| format!("écriture de {} : {e}", path.display()))?;
    // installée pour tout le PC (Program Files) ou pour l'utilisateur seul : on garde le même mode
    let exe = exe_path().unwrap_or_default().to_string_lossy().to_lowercase();
    let pf = std::env::var("ProgramFiles").unwrap_or_default().to_lowercase();
    let mode = if !pf.is_empty() && exe.starts_with(&pf) { "/ALLUSERS" } else { "/CURRENTUSER" };
    std::process::Command::new(&path).args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", mode]).spawn().map_err(|e| format!("lancement de l'installeur : {e}"))?;
    Ok(())
}

fn install_portable(r: &Release) -> Result<(), String> {
    let url = r.zip_url.as_deref().ok_or("cette release n'a pas de version portable")?;
    let data = download(url, &r.version)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| format!("zip illisible : {e}"))?;
    let mut exe_bytes = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.name().replace('\\', "/").to_lowercase().ends_with("dragowalker.exe") {
            let mut v = Vec::new();
            f.read_to_end(&mut v).map_err(|e| e.to_string())?;
            exe_bytes = Some(v);
            break;
        }
    }
    let exe_bytes = exe_bytes.ok_or("DragoWalker.exe absent du zip")?;
    let cur = exe_path().ok_or("emplacement de l'exe inconnu")?;
    let dir = cur.parent().ok_or("dossier de l'exe inconnu")?;
    let new = dir.join("DragoWalker.new.exe");
    let old = dir.join("DragoWalker.old.exe");
    std::fs::write(&new, exe_bytes).map_err(|e| format!("écriture dans {} : {e} (dossier protégé ?)", dir.display()))?;
    let _ = std::fs::remove_file(&old);
    // Windows permet de renommer un exe en cours d'exécution, pas de le remplacer
    std::fs::rename(&cur, &old).map_err(|e| format!("renommage de l'exe : {e}"))?;
    if let Err(e) = std::fs::rename(&new, &cur) {
        let _ = std::fs::rename(&old, &cur);
        return Err(format!("remplacement de l'exe : {e}"));
    }
    std::process::Command::new(&cur).spawn().map_err(|e| format!("redémarrage : {e}"))?;
    Ok(())
}

/// Bandeau en haut de la fenêtre quand une mise à jour est disponible ou en cours.
pub fn banner(ctx: &egui::Context) {
    let s = state().lock().map(|g| g.clone()).unwrap_or(State::Hidden);
    let show = matches!(s, State::Available(_) | State::Downloading(..) | State::Error(_));
    if !show {
        if matches!(s, State::Checking) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        return;
    }
    egui::TopBottomPanel::top("update_banner").show(ctx, |ui| {
        ui.horizontal(|ui| match &s {
            State::Available(r) => {
                ui.label(egui::RichText::new(format!("Nouvelle version {} disponible (vous avez la {}).", r.version, current())).strong());
                let can = if is_installed() { r.setup_url.is_some() } else { r.zip_url.is_some() };
                if can && ui.button("Mettre à jour maintenant").on_hover_text("Télécharge la nouvelle version, l'installe et redémarre DragoWalker. Enregistrez votre projet avant.").clicked() {
                    install(r.clone(), Some(ctx.clone()));
                }
                if !r.page.is_empty() {
                    ui.hyperlink_to("Voir les nouveautés", &r.page);
                }
                if ui.button("Plus tard").clicked() {
                    set(State::Hidden);
                }
            }
            State::Downloading(v, p) => {
                ui.label(format!("Téléchargement de la version {v}…"));
                ui.add(egui::ProgressBar::new(*p).show_percentage().desired_width(220.0));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            State::Error(e) => {
                ui.colored_label(egui::Color32::from_rgb(230, 90, 80), format!("Mise à jour impossible : {e}"));
                if ui.button("Réessayer").clicked() {
                    check();
                }
                if ui.button("Fermer").clicked() {
                    set(State::Hidden);
                }
            }
            _ => {}
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaison_des_versions() {
        assert!(is_newer("v0.2.2", "0.2.1"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("v0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
    }
}
