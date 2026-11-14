//! Serveur FiveM de test, installé et piloté par DragoWalker.
//!
//! - Installation : le serveur officiel FiveM pour Windows (artefact « recommended » annoncé par
//!   changelogs-live.fivem.net/api/changelog/versions/win32/server, un .zip) est téléchargé et décompressé dans
//!   un dossier à DragoWalker. Rien d'autre : pas de base de données, pas d'ESX, pas de txAdmin.
//! - Configuration minimale (server.cfg) : en local seulement (127.0.0.1:30120), 4 joueurs, la map du projet
//!   et la ressource d'outils « dw_tools » (apparition, noclip, téléportation, coordonnées, rechargement).
//! - « Tester en jeu » : export de la map dans le dossier resources du serveur, démarrage du serveur, puis
//!   FiveM se connecte tout seul (lien fivem://connect/127.0.0.1:30120).
//! - Recharger sans se déconnecter : DragoWalker écrit « dw_reload » dans la console du serveur ; dw_tools
//!   éloigne d'abord la zone chargée par le jeu (SetFocusPosAndVel) pour qu'il lâche les collisions et
//!   l'intérieur, relance la map (restart), puis ramène la zone (ClearFocus), redemande les collisions
//!   (RequestCollisionAtCoord) et recharge l'intérieur (RefreshInterior). Même chose en jeu avec /dw_refresh.
//! - Une clé de licence Cfx (gratuite, portal.cfx.re) est obligatoire pour tout serveur FiveM.

use crate::app::DwApp;
use crate::math::V3;
use eframe::egui::{self, Color32, RichText};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const PORT: u16 = 30120;
const VERSIONS_URL: &str = "https://changelogs-live.fivem.net/api/changelog/versions/win32/server";
const KEYS_URL: &str = "https://portal.cfx.re/servers/registration-keys";
const TOOLS: &str = "dw_tools";
/// Lignes gardées de la console du serveur.
const CONSOLE_LINES: usize = 400;

/// Ressource de la communauté à cocher dans le serveur de test (téléchargée depuis GitHub au premier usage).
pub struct Addon {
    /// nom du dossier de la ressource dans resources/ (et de la commande ensure / stop)
    pub key: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
    /// dépôt GitHub (propriétaire/nom) et branche
    pub repo: &'static str,
    pub branch: &'static str,
    /// sous-dossier du dépôt qui est la ressource ("" = la racine)
    pub sub: &'static str,
    /// parties du dépôt inutiles au serveur (outils, images)
    pub skip: &'static [&'static str],
}

pub const ADDONS: [Addon; 2] = [
    Addon {
        key: "limits",
        label: "Pas de limite au bord de la carte (limits)",
        tip: "iModyHK/limits : au bord de la carte, le véhicule ne tombe plus en panne — utile pour une map ajoutée hors de la carte d'origine",
        repo: "iModyHK/limits",
        branch: "master",
        sub: "limits",
        skip: &[],
    },
    Addon {
        key: "ls-removed",
        label: "Los Santos retiré (ls-removed)",
        tip: "alexlicuriceanu/ls-removed : carte du jeu vide (bâtiments, LOD, circulation, PNJ de Los Santos retirés), pour tester une map custom seule ; son stream.zip est décompressé automatiquement. L'activer ou le couper pendant le jeu peut demander de se reconnecter.",
        repo: "alexlicuriceanu/ls-removed",
        branch: "main",
        sub: "",
        skip: &["generator_scripts/", "images/"],
    },
];

/// Parties de la map qu'on peut laisser de côté pour un test (pour trouver ce qui fait planter le jeu) :
/// (clé, libellé). Les MLO importés s'y ajoutent (« mlo:<nom> »).
pub const TEST_PARTS: [(&str, &str); 4] = [("peds", "PNJ (dw_peds.lua)"), ("traffic", "Voitures de PNJ (dw_traffic.lua)"), ("veg", "Végétation posée au pinceau"), ("water", "Eau (dw_water.lua)")];

/// Pools du jeu touchés par chaque partie (tailles lues dans update.rpf\common\data\gameconfig.xml du jeu
/// installé ; les seuls pools de taille 256 sont Peds, CPropManagementHelper, CTaskUseScenarioEntityExtension et
/// CPathNodeRouteSearchHelper). Affiché à côté des cases : un crash « Pool Full, Size == 256 » vise d'abord les
/// PNJ, puis les voitures.
pub fn part_pools(key: &str) -> &'static str {
    match key {
        "peds" => "pools Peds, CPropManagementHelper, CTaskUseScenarioEntityExtension : 256 chacun",
        "traffic" => "pools Peds : 256, CPathNodeRouteSearchHelper : 256",
        "veg" => "pools Dummy Object : 10010, Object : 2300",
        "water" => "pas de pool d'objets",
        _ => "pools InteriorInst : 150, PortalInst : 200, InteriorProxy : 1250",
    }
}

/// Suite d'une conversion en arrière-plan.
#[derive(Clone, Copy)]
pub enum After {
    Start,
    Reload,
    /// export manuel (menu Fichier) : compte rendu seulement ; dossier et début du compte rendu dans
    /// `ServerState::export_info`
    Export,
}

/// Conversion en cours : (résultat quand elle est finie, suite, début).
pub type Job = (Arc<Mutex<Option<(bool, Vec<String>)>>>, After, std::time::Instant);

pub struct ServerState {
    pub open: bool,
    pub job: Option<Job>,
    /// les scripts de dw_tools ont changé depuis le démarrage du serveur
    pub tools_changed: bool,
    /// parties laissées de côté pendant les tests (réglage server_skip)
    pub skip: Vec<String>,
    /// ressources de la communauté cochées (réglage server_addons)
    pub addons_on: Vec<String>,
    /// ressources de la communauté démarrées sur le serveur en marche
    pub addons_live: Vec<String>,
    pub addon_msg: Arc<Mutex<String>>,
    pub addon_busy: Arc<Mutex<bool>>,
    pub dir: String,
    pub license: String,
    /// message d'installation (téléchargement, décompression)
    pub install_msg: Arc<Mutex<String>>,
    pub installing: Arc<Mutex<bool>>,
    pub child: Option<std::process::Child>,
    pub stdin: Option<std::process::ChildStdin>,
    pub console: Arc<Mutex<Vec<String>>>,
    pub command: String,
    /// nom de la ressource de la map (dossier exporté)
    pub resource: String,
    /// test en jeu en cours : un plantage de FiveM écrit après cet instant ouvre l'analyse
    pub watch: Option<std::time::SystemTime>,
    pub watch_at: f64,
    /// export manuel en cours de conversion : (dossier exporté, ressource FiveM, compte rendu de l'export)
    pub export_info: Option<(PathBuf, bool, String)>,
}

impl ServerState {
    pub fn new() -> ServerState {
        let s = crate::app::load_settings();
        let base = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        let dir = s["server_dir"].as_str().map(|x| x.to_string()).unwrap_or_else(|| base.join("Documents").join("DragoWalker").join("ServeurFiveM").to_string_lossy().to_string());
        ServerState {
            open: std::env::var("DW_SERVER_WINDOW").is_ok(),
            job: None,
            tools_changed: false,
            skip: s["server_skip"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|x| x.to_string())).collect()).unwrap_or_default(),
            addons_on: s["server_addons"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|x| x.to_string())).collect()).unwrap_or_default(),
            addons_live: Vec::new(),
            addon_msg: Arc::new(Mutex::new(String::new())),
            addon_busy: Arc::new(Mutex::new(false)),
            dir,
            license: s["server_license"].as_str().unwrap_or("").to_string(),
            install_msg: Arc::new(Mutex::new(String::new())),
            installing: Arc::new(Mutex::new(false)),
            child: None,
            stdin: None,
            console: Arc::new(Mutex::new(Vec::new())),
            command: String::new(),
            resource: String::new(),
            watch: None,
            watch_at: 0.0,
            export_info: None,
        }
    }

    pub fn exe(&self) -> PathBuf {
        Path::new(&self.dir).join("artifacts").join("FXServer.exe")
    }
    pub fn data(&self) -> PathBuf {
        Path::new(&self.dir).join("server-data")
    }
    pub fn installed(&self) -> bool {
        self.exe().is_file()
    }
    pub fn running(&mut self) -> bool {
        match self.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.child = None;
                self.stdin = None;
                false
            }
            None => false,
        }
    }

    /// Télécharge et décompresse le serveur officiel (dans un fil à part).
    pub fn install(&self) {
        if *self.installing.lock().unwrap_or_else(|e| e.into_inner()) {
            return;
        }
        *self.installing.lock().unwrap_or_else(|e| e.into_inner()) = true;
        let (msg, busy, dir) = (self.install_msg.clone(), self.installing.clone(), PathBuf::from(&self.dir));
        std::thread::spawn(move || {
            let set = |m: String| *msg.lock().unwrap_or_else(|e| e.into_inner()) = m;
            let r = (|| -> Result<String, String> {
                set("Recherche de la version recommandée du serveur FiveM…".into());
                let v: serde_json::Value = ureq::get(VERSIONS_URL).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
                let url = v["recommended_download"].as_str().ok_or("adresse du serveur introuvable")?.to_string();
                let ver = v["recommended"].as_str().unwrap_or("?").to_string();
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let zip_path = dir.join("server.zip");
                let resp = ureq::get(&url).call().map_err(|e| e.to_string())?;
                let total: u64 = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);
                let mut rd = resp.into_reader();
                let mut f = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; 1 << 16];
                let mut got = 0u64;
                loop {
                    let n = rd.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    got += n as u64;
                    if total > 0 {
                        set(format!("Téléchargement du serveur FiveM {ver} : {} %", got * 100 / total));
                    }
                }
                drop(f);
                set("Décompression…".into());
                let art = dir.join("artifacts");
                let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
                let mut z = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                for i in 0..z.len() {
                    let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                    let Some(rel) = e.enclosed_name() else { continue };
                    let out = art.join(rel);
                    if e.is_dir() {
                        let _ = std::fs::create_dir_all(&out);
                        continue;
                    }
                    if let Some(p) = out.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let mut o = std::fs::File::create(&out).map_err(|e| e.to_string())?;
                    std::io::copy(&mut e, &mut o).map_err(|e| e.to_string())?;
                }
                let _ = std::fs::remove_file(&zip_path);
                Ok(format!("Serveur FiveM {ver} installé dans {}.", dir.display()))
            })();
            set(match r {
                Ok(m) => m,
                Err(e) => format!("Installation impossible : {e}"),
            });
            *busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
        });
    }

    pub fn addon_installed(&self, key: &str) -> bool {
        self.data().join("resources").join(key).join("fxmanifest.lua").is_file()
    }

    /// Ressources cochées et installées, dans l'ordre de la liste (démarrées avant la map).
    pub fn addons_active(&self) -> Vec<&'static str> {
        ADDONS.iter().filter(|a| self.addons_on.iter().any(|k| k == a.key) && self.addon_installed(a.key)).map(|a| a.key).collect()
    }

    /// Télécharge une ressource de la communauté depuis GitHub (archive .zip de la branche) dans resources/<clé>,
    /// sans les parties inutiles ; un stream/stream.zip est décompressé dans stream/ (FiveM ne lit pas les .zip).
    pub fn install_addon(&self, key: &'static str) {
        let Some(a) = ADDONS.iter().find(|a| a.key == key) else { return };
        if *self.addon_busy.lock().unwrap_or_else(|e| e.into_inner()) {
            return;
        }
        *self.addon_busy.lock().unwrap_or_else(|e| e.into_inner()) = true;
        let (msg, busy) = (self.addon_msg.clone(), self.addon_busy.clone());
        let dest = self.data().join("resources").join(a.key);
        let (repo, branch, sub, skip, label) = (a.repo, a.branch, a.sub, a.skip, a.label);
        std::thread::spawn(move || {
            let set = |m: String| *msg.lock().unwrap_or_else(|e| e.into_inner()) = m;
            let r = (|| -> Result<usize, String> {
                set(format!("Téléchargement de {repo}…"));
                let url = format!("https://codeload.github.com/{repo}/zip/refs/heads/{branch}");
                let mut data = Vec::new();
                ureq::get(&url).call().map_err(|e| e.to_string())?.into_reader().read_to_end(&mut data).map_err(|e| e.to_string())?;
                let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
                let _ = std::fs::create_dir_all(&dest);
                let mut n = 0;
                for i in 0..z.len() {
                    let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                    let Some(path) = e.enclosed_name() else { continue };
                    // premier dossier de l'archive : « <dépôt>-<branche> »
                    let rel: PathBuf = path.components().skip(1).collect();
                    let rel_s = rel.to_string_lossy().replace('\\', "/");
                    let rel_s = if sub.is_empty() {
                        rel_s
                    } else if let Some(r) = rel_s.strip_prefix(&format!("{sub}/")) {
                        r.to_string()
                    } else {
                        continue;
                    };
                    if rel_s.is_empty() || skip.iter().any(|s| rel_s.starts_with(s) || rel_s == s.trim_end_matches('/')) {
                        continue;
                    }
                    let out = dest.join(&rel_s);
                    if e.is_dir() {
                        let _ = std::fs::create_dir_all(&out);
                        continue;
                    }
                    if let Some(p) = out.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let mut o = std::fs::File::create(&out).map_err(|e| e.to_string())?;
                    std::io::copy(&mut e, &mut o).map_err(|e| e.to_string())?;
                    n += 1;
                }
                let sz = dest.join("stream").join("stream.zip");
                if sz.is_file() {
                    set(format!("Décompression du stream de {repo}…"));
                    let f = std::fs::File::open(&sz).map_err(|e| e.to_string())?;
                    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
                    let sd = dest.join("stream");
                    for i in 0..z.len() {
                        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                        let Some(rel) = e.enclosed_name() else { continue };
                        let out = sd.join(rel);
                        if e.is_dir() {
                            let _ = std::fs::create_dir_all(&out);
                            continue;
                        }
                        if let Some(p) = out.parent() {
                            let _ = std::fs::create_dir_all(p);
                        }
                        let mut o = std::fs::File::create(&out).map_err(|e| e.to_string())?;
                        std::io::copy(&mut e, &mut o).map_err(|e| e.to_string())?;
                        n += 1;
                    }
                    let _ = std::fs::remove_file(&sz);
                }
                Ok(n)
            })();
            set(match r {
                Ok(n) => format!("{label} installé ({n} fichiers, depuis github.com/{repo}).", ),
                Err(e) => format!("Téléchargement de {repo} impossible : {e}"),
            });
            *busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
        });
    }

    /// Écrit server.cfg et la ressource d'outils.
    pub fn write_config(&mut self, spawn: V3) -> Result<(), String> {
        let data = self.data();
        let tools = data.join("resources").join(TOOLS);
        std::fs::create_dir_all(&tools).map_err(|e| e.to_string())?;
        // nouvelle version des outils (mise à jour de DragoWalker) : à redémarrer sur un serveur en marche
        let old = std::fs::read_to_string(tools.join("client.lua")).unwrap_or_default() + &std::fs::read_to_string(tools.join("server.lua")).unwrap_or_default();
        if old != format!("{TOOLS_CLIENT}{TOOLS_SERVER}") {
            self.tools_changed = true;
        }
        std::fs::write(tools.join("fxmanifest.lua"), TOOLS_MANIFEST).map_err(|e| e.to_string())?;
        std::fs::write(tools.join("client.lua"), TOOLS_CLIENT).map_err(|e| e.to_string())?;
        std::fs::write(tools.join("server.lua"), TOOLS_SERVER).map_err(|e| e.to_string())?;
        let cfg = format!(
            "# Serveur de test DragoWalker (local seulement)\n\
             endpoint_add_tcp \"127.0.0.1:{PORT}\"\n\
             endpoint_add_udp \"127.0.0.1:{PORT}\"\n\
             sv_hostname \"DragoWalker - test\"\n\
             sv_maxclients 4\n\
             sv_endpointprivacy true\n\
             sv_scriptHookAllowed 0\n\
             sv_licenseKey \"{}\"\n\
             setr dw_spawn \"{} {} {}\"\n\
             set dw_map \"{}\"\n\
             # dw_tools relance la map (restart) pendant le rechargement\n\
             add_ace resource.{TOOLS} command.restart allow\n\
             add_ace resource.{TOOLS} command.ensure allow\n\
             ensure {TOOLS}\n{}{}",
            self.license.trim(),
            spawn.x,
            spawn.y,
            spawn.z + 1.0,
            self.resource,
            // ressources de la communauté cochées, avant la map
            self.addons_active().iter().map(|k| format!("ensure {k}\n")).collect::<String>(),
            if self.resource.is_empty() { String::new() } else { format!("ensure {}\n", self.resource) }
        );
        std::fs::write(data.join("server.cfg"), cfg).map_err(|e| e.to_string())
    }

    /// Démarre le serveur (console lue dans un fil).
    pub fn start(&mut self) -> Result<(), String> {
        let mut cmd = std::process::Command::new(self.exe());
        // la console est lue sur l'entrée standard (un tuyau ici, pas un terminal) : FXServer n'attend un terminal
        // que si con_disableNonTTYReads est vrai (citizen-server-impl.dll, FUN_18016f0d0 ; défaut 0 posé par
        // FUN_18016de30) — mis à faux explicitement ; chaque ligne lue est une commande
        cmd.current_dir(self.data()).args(["+set", "con_disableNonTTYReads", "false", "+exec", "server.cfg"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Démarrage du serveur impossible : {e}"))?;
        for out in [child.stdout.take().map(|o| Box::new(o) as Box<dyn Read + Send>), child.stderr.take().map(|o| Box::new(o) as Box<dyn Read + Send>)].into_iter().flatten() {
            let console = self.console.clone();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    // codes de couleur de la console (ESC[…m) retirés
                    let mut clean = String::new();
                    let mut esc = false;
                    for ch in line.chars() {
                        if ch == '\u{1b}' {
                            esc = true;
                        } else if esc {
                            if ch.is_ascii_alphabetic() {
                                esc = false;
                            }
                        } else {
                            clean.push(ch);
                        }
                    }
                    if let Ok(mut c) = console.lock() {
                        c.push(clean);
                        let n = c.len();
                        if n > CONSOLE_LINES {
                            c.drain(0..n - CONSOLE_LINES);
                        }
                    }
                }
            });
        }
        self.stdin = child.stdin.take();
        self.child = Some(child);
        self.addons_live = self.addons_active().iter().map(|k| k.to_string()).collect();
        Ok(())
    }

    /// Envoie une commande à la console du serveur.
    pub fn send(&mut self, cmd: &str) -> bool {
        let ok = self.stdin.as_mut().map(|s| s.write_all(format!("{cmd}\n").as_bytes()).and_then(|_| s.flush()).is_ok()).unwrap_or(false);
        if ok {
            if let Ok(mut c) = self.console.lock() {
                c.push(format!("> {cmd}"));
            }
        }
        ok
    }

    pub fn stop(&mut self) {
        self.send("quit");
        if let Some(mut c) = self.child.take() {
            std::thread::sleep(std::time::Duration::from_millis(800));
            if matches!(c.try_wait(), Ok(None)) {
                let _ = c.kill();
            }
        }
        self.stdin = None;
    }
}

impl Drop for ServerState {
    /// DragoWalker fermé : le serveur de test s'arrête aussi.
    fn drop(&mut self) {
        if self.child.is_some() {
            self.stop();
        }
    }
}

/// Ouvre FiveM et le connecte au serveur local.
pub fn connect() -> Result<(), String> {
    // FiveM refuse d'être lancé par un autre programme (« This application should be launched directly from the
    // shell or a web browser » quand il venait de cmd /C start) : le lien est confié à l'Explorateur Windows, qui
    // le passe au bureau déjà ouvert — FiveM est alors lancé par le shell, comme après un clic dans un navigateur.
    // FiveM.exe 0x14036b2a0 / FUN_1400bde50 : erreur connue (pas de rapport de plantage) ; voir MECHANICS.md
    let url = format!("fivem://connect/127.0.0.1:{PORT}");
    std::process::Command::new("explorer.exe").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

impl DwApp {
    /// Exporte la map dans le dossier resources du serveur. Rend le compte rendu de l'export.
    fn server_export(&mut self) -> Result<String, String> {
        let res_dir = self.server.data().join("resources");
        std::fs::create_dir_all(&res_dir).map_err(|e| e.to_string())?;
        // même nom de dossier que l'export (export::resource_name : minuscules, sans accent ni caractère spécial)
        let name = crate::export::resource_name(&self.proj.name);
        if name == TOOLS {
            return Err("Change le nom du projet (dw_tools est réservé).".into());
        }
        self.server.resource = name;
        // parties décochées dans « Contenu du test » : laissées de côté pour cet export seulement
        self.export_skip = self.server.skip.clone();
        // la conversion par CodeWalker (la partie longue) est laissée au fil du serveur
        self.export_defer_convert = true;
        self.pending_convert = None;
        let rep = self.export_project(&res_dir, true);
        self.export_defer_convert = false;
        self.export_skip.clear();
        Ok(rep)
    }

    /// Lance la conversion en arrière-plan ; `then` : ce qu'il faudra faire quand elle sera finie.
    fn server_convert_then(&mut self, then: After) {
        match self.pending_convert.take() {
            Some((dll, stream, out)) => {
                let slot: Arc<Mutex<Option<(bool, Vec<String>)>>> = Arc::new(Mutex::new(None));
                let s2 = slot.clone();
                std::thread::spawn(move || {
                    let r = crate::export::convert_and_tidy(&dll, &stream, &out);
                    *s2.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
                });
                self.server.job = Some((slot, then, std::time::Instant::now()));
            }
            // rien à convertir (CodeWalker introuvable) : tout de suite
            None => self.server_after(then, Vec::new()),
        }
    }

    /// Export manuel : conversion de `pending_convert` dans un fil à part (même chemin que le test en jeu).
    pub fn server_convert_export(&mut self, out: PathBuf, fivem: bool, export_report: String) {
        self.server.export_info = Some((out, fivem, export_report));
        self.server_convert_then(After::Export);
    }

    /// Fin de la conversion : démarre le serveur ou recharge la map.
    fn server_after(&mut self, then: After, rep: Vec<String>) {
        let mut rep = rep.join(" ");
        if let After::Export = then {
            let (out, fivem, before) = self.server.export_info.take().unwrap_or_default();
            if fivem {
                let scan = crate::check::scan_resource_dir(&out);
                let problems: Vec<String> = scan.iter().filter(|r| r.0 == crate::check::Level::Error).map(|r| r.1.clone()).collect();
                if !problems.is_empty() {
                    rep += &format!(" ⚠ Contrôle de l'export : {}", problems.join(" · "));
                }
                self.check_folder_report = scan;
            }
            let before = before.replace("Conversion en binaires du jeu en cours (en arrière-plan)…", "").trim().to_string();
            self.status = format!("Export terminé. {before} {rep}");
            return;
        }
        // contrôle du dossier exporté : ce que le jeu ne pourra pas charger
        let scan = crate::check::scan_resource_dir(&self.server.data().join("resources").join(&self.server.resource));
        let problems: Vec<String> = scan.iter().filter(|r| r.0 == crate::check::Level::Error).map(|r| r.1.clone()).collect();
        if !problems.is_empty() {
            rep += &format!(" ⚠ Contrôle de l'export : {}", problems.join(" · "));
        }
        self.check_folder_report = scan;
        self.server.watch = Some(std::time::SystemTime::now());
        match then {
            After::Reload => {
                let r = self.server.resource.clone();
                // la map vient peut-être d'être exportée sous un nouveau nom : nouveau scan avant ensure
                self.server.send("refresh");
                if std::mem::take(&mut self.server.tools_changed) {
                    // ensure plutôt que restart : restart ne relance qu'une ressource démarrée (état 3) et répond
                    // « Can't restart a stopped resource. » sinon (citizen-server-impl.dll FUN_180155de0) ; ensure
                    // arrête si elle tourne puis démarre toujours (FUN_180156010)
                    self.server.send(&format!("ensure {TOOLS}"));
                }
                self.server.send(&format!("ensure {r}"));
                self.server.send("dw_reload");
                self.status = format!("Map rechargée sur le serveur (tu restes connecté). {rep}");
            }
            After::Start => match self.server.start() {
                Ok(_) => {
                    let c = connect().err().map(|e| format!(" FiveM n'a pas pu être ouvert ({e}) : connecte-toi à 127.0.0.1:{PORT}.")).unwrap_or_default();
                    self.status = format!("Serveur de test démarré, FiveM se connecte.{c} {rep}");
                }
                Err(e) => self.status = e,
            },
            // traité plus haut
            After::Export => {}
        }
    }

    /// À chaque image : conversion finie → suite (démarrage ou rechargement).
    pub fn server_tick(&mut self, ctx: &egui::Context) {
        // test en jeu : FiveM a-t-il planté ?
        if let Some(since) = self.server.watch {
            let now = ctx.input(|i| i.time);
            if now - self.server.watch_at > 4.0 {
                self.server.watch_at = now;
                if let Some(log) = crate::crashlog::crashed_since(since) {
                    self.server.watch = None;
                    if let Ok(a) = crate::crashlog::analyse_folder(log.parent().unwrap_or(std::path::Path::new("."))) {
                        self.crash.analysis = Some(a);
                        self.crash.error.clear();
                        self.crash.open = true;
                        self.status = "FiveM a planté pendant le test : l'analyse est ouverte.".into();
                    }
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(4));
        }
        let Some((slot, then, t0)) = self.server.job.as_ref().map(|j| (j.0.clone(), j.1, j.2)) else { return };
        let done = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        match done {
            Some((_, rep)) => {
                self.server.job = None;
                self.server_after(then, rep);
            }
            None => {
                self.status = format!("Conversion des fichiers par CodeWalker… {} s (DragoWalker reste utilisable)", t0.elapsed().as_secs());
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }

    /// « Tester en jeu » : export, serveur démarré si besoin, FiveM connecté ; serveur déjà lancé : rechargement.
    pub fn server_test(&mut self) {
        if !self.server.installed() {
            self.status = "Installe d'abord le serveur de test (fenêtre Serveur de test).".into();
            return;
        }
        if self.server.license.trim().is_empty() {
            self.status = "Il faut une clé de licence Cfx (gratuite, portal.cfx.re) pour lancer un serveur FiveM.".into();
            return;
        }
        if self.server.job.is_some() {
            self.status = "Une conversion est déjà en cours, attends qu'elle finisse.".into();
            return;
        }
        if let Err(e) = self.server_export() {
            self.status = e;
            return;
        }
        let spawn = self.cam.target;
        if let Err(e) = self.server.write_config(spawn) {
            self.status = format!("Configuration du serveur impossible : {e}");
            return;
        }
        let then = if self.server.running() { After::Reload } else { After::Start };
        self.server_convert_then(then);
    }

    /// Recharge la map sans déconnecter le joueur (même chemin que « Tester en jeu » quand le serveur tourne).
    #[allow(dead_code)]
    pub fn server_reload(&mut self) {
        self.server_test();
    }

    /// Fenêtre « Serveur de test ».
    pub fn server_window(&mut self, ctx: &egui::Context) {
        if !self.server.open {
            return;
        }
        let mut open = true;
        let mut act: Option<&'static str> = None;
        self.show_panel(ctx, "serveur", "Serveur de test FiveM", crate::docking::Dock::Floating, [520.0, 560.0], &mut open, |s, ui| {
            let running = s.server.running();
            ui.label(RichText::new("Un serveur FiveM sur ton PC, géré par DragoWalker : teste ta map et tes MLO en jeu, recharge-les sans te déconnecter.").small());
            ui.horizontal(|ui| {
                ui.label("Dossier");
                let r = ui.add(egui::TextEdit::singleline(&mut s.server.dir).desired_width(300.0));
                if r.lost_focus() {
                    crate::app::save_setting("server_dir", serde_json::json!(s.server.dir));
                }
            });
            if s.server.installed() {
                ui.label(RichText::new("✔ Serveur FiveM installé").color(Color32::from_rgb(140, 210, 140)));
            } else {
                ui.horizontal(|ui| {
                    let busy = *s.server.installing.lock().unwrap_or_else(|e| e.into_inner());
                    if ui.add_enabled(!busy, egui::Button::new(RichText::new("Installer le serveur (≈ 100 Mo)").strong())).on_hover_text("Télécharge la version recommandée du serveur officiel FiveM pour Windows").clicked() {
                        act = Some("install");
                    }
                });
            }
            let m = s.server.install_msg.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if !m.is_empty() {
                ui.label(RichText::new(m).small());
            }
            ui.horizontal(|ui| {
                ui.label("Clé de licence");
                let r = ui.add(egui::TextEdit::singleline(&mut s.server.license).password(true).desired_width(230.0).hint_text("cfxk_…"));
                if r.lost_focus() {
                    crate::app::save_setting("server_license", serde_json::json!(s.server.license));
                }
                if ui.small_button("Obtenir une clé").on_hover_text(KEYS_URL).clicked() {
                    let _ = std::process::Command::new("explorer").arg(KEYS_URL).spawn();
                }
            });
            ui.separator();
            ui.label(RichText::new("Ressources à cocher").strong());
            let busy = *s.server.addon_busy.lock().unwrap_or_else(|e| e.into_inner());
            for a in ADDONS.iter() {
                let mut on = s.server.addons_on.iter().any(|k| k == a.key);
                let inst = s.server.addon_installed(a.key);
                ui.horizontal(|ui| {
                    if ui.add_enabled(s.server.installed() && (inst || !busy), egui::Checkbox::new(&mut on, a.label)).on_hover_text(a.tip).changed() {
                        s.server.addons_on.retain(|k| k != a.key);
                        if on {
                            s.server.addons_on.push(a.key.to_string());
                            if !inst {
                                s.server.install_addon(a.key);
                            }
                        }
                        crate::app::save_setting("server_addons", serde_json::json!(s.server.addons_on));
                    }
                    if !inst {
                        ui.label(RichText::new("téléchargé depuis GitHub au premier usage").small().weak());
                    }
                });
            }
            let am = s.server.addon_msg.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if !am.is_empty() {
                ui.label(RichText::new(am).small());
            }
            ui.separator();
            ui.label(RichText::new("Contenu du test").strong()).on_hover_text("Décoche une partie pour la laisser de côté au prochain test (pas dans l'export normal) : utile pour trouver ce qui fait planter le jeu, une partie à la fois, avec « Réexporter et recharger »");
            let mut parts: Vec<(String, String)> = TEST_PARTS.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
            parts.extend(s.proj.mlo_res.iter().map(|r| (format!("mlo:{}", r.name), format!("MLO « {} »", r.name))));
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (k, l) in &parts {
                    let mut on = !s.server.skip.contains(k);
                    if ui.checkbox(&mut on, l.as_str()).on_hover_text(part_pools(k)).changed() {
                        s.server.skip.retain(|x| x != k);
                        if !on {
                            s.server.skip.push(k.clone());
                        }
                        changed = true;
                    }
                }
            });
            if changed {
                crate::app::save_setting("server_skip", serde_json::json!(s.server.skip));
            }
            if !s.server.skip.is_empty() {
                ui.label(RichText::new(format!("Laissé de côté au prochain test : {}", s.server.skip.join(", "))).small().color(Color32::from_rgb(255, 190, 110)));
            }
            ui.label(RichText::new("Crash « Pool Full, Size == 256 » : dans GTA V, seuls les pools des PNJ et de leurs activités (Peds, CPropManagementHelper, CTaskUseScenarioEntityExtension) et des recherches de trajet des voitures (CPathNodeRouteSearchHelper) ont 256 places — essaie d'abord sans PNJ, puis sans voitures de PNJ.").small().weak());
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let converting = s.server.job.is_some();
                let can = s.server.installed() && !s.server.license.trim().is_empty() && !converting;
                let label = if converting { "Conversion en cours…" } else if running { "Réexporter et recharger" } else { "Lancer le test" };
                if ui.add_enabled(can, egui::Button::new(RichText::new(label).strong())).on_hover_text("Exporte la map dans le serveur, le démarre et connecte FiveM ; s'il tourne déjà, recharge la map sans te déconnecter").clicked() {
                    act = Some("test");
                }
                if ui.add_enabled(running, egui::Button::new("Aller ici en jeu")).on_hover_text("Téléporte ton personnage au centre de la vue de DragoWalker").clicked() {
                    act = Some("goto");
                }
                if ui.add_enabled(running, egui::Button::new("Reconnecter FiveM")).clicked() {
                    act = Some("connect");
                }
                if ui.add_enabled(running, egui::Button::new("Arrêter le serveur")).clicked() {
                    act = Some("stop");
                }
                if ui.button("Analyser un plantage").on_hover_text("Explique le dernier plantage de FiveM d'après son journal").clicked() {
                    act = Some("crash");
                }
            });
            ui.label(RichText::new(format!("En jeu : F2 noclip (Z Q S D, Espace / Ctrl, Maj plus vite) · /tp x y z · /tpm vers le repère · /dw_home · /dw_coords · /dw_copy · /dw_refresh recharge la zone. État : {}", if running { "serveur en marche" } else { "arrêté" })).small().weak());
            ui.separator();
            ui.label(RichText::new("Console du serveur").strong());
            let lines = s.server.console.lock().map(|c| c.clone()).unwrap_or_default();
            egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).id_salt("srv_console").show(ui, |ui| {
                for l in lines.iter().rev().take(200).rev() {
                    ui.label(RichText::new(l).monospace().size(11.0));
                }
            });
            ui.horizontal(|ui| {
                let r = ui.add_enabled(running, egui::TextEdit::singleline(&mut s.server.command).desired_width(360.0).hint_text("commande (ex. restart nom_de_la_map)"));
                if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.add_enabled(running, egui::Button::new("Envoyer")).clicked() {
                    act = Some("cmd");
                }
            });
        });
        self.server.open = open;
        // serveur en marche : les ressources cochées démarrent, les décochées s'arrêtent tout de suite
        if self.server.running() {
            let want = self.server.addons_active();
            let new: Vec<&str> = want.iter().filter(|k| !self.server.addons_live.iter().any(|l| l == *k)).copied().collect();
            if !new.is_empty() {
                // un dossier ajouté après le démarrage n'est connu du serveur qu'après un nouveau scan des
                // ressources (« refresh » : citizen-server-impl.dll FUN_1801509d0, « Found %d new resources… ») ;
                // sinon ensure / stop répondent « Couldn't find resource » (FUN_180155b10)
                self.server.send("refresh");
            }
            for k in new {
                self.server.send(&format!("ensure {k}"));
                self.server.addons_live.push(k.to_string());
            }
            for k in self.server.addons_live.clone() {
                if !want.contains(&k.as_str()) {
                    self.server.send(&format!("stop {k}"));
                    self.server.addons_live.retain(|l| l != &k);
                }
            }
        }
        match act {
            Some("install") => self.server.install(),
            Some("crash") => self.crash.open = true,
            Some("test") => self.server_test(),
            Some("goto") => {
                let p = self.cam.target;
                self.server.send(&format!("dw_tp {} {} {}", p.x, p.y, p.z + 1.0));
                self.status = format!("Téléportation en jeu vers {:.1} {:.1} {:.1}.", p.x, p.y, p.z);
            }
            Some("connect") => {
                let _ = connect();
            }
            Some("stop") => {
                self.server.stop();
                self.status = "Serveur de test arrêté.".into();
            }
            Some("cmd") => {
                let c = std::mem::take(&mut self.server.command);
                if !c.trim().is_empty() {
                    self.server.send(c.trim());
                }
            }
            _ => {}
        }
    }
}

const TOOLS_MANIFEST: &str = "fx_version 'cerulean'\ngame 'gta5'\n\ndescription 'Outils de test DragoWalker : apparition, noclip, téléportation, coordonnées, rechargement'\n\nclient_script 'client.lua'\nserver_script 'server.lua'\n";

const TOOLS_SERVER: &str = r#"-- DragoWalker : rechargement de la map sans déconnecter les joueurs
local waiting = false

local function mapName() return GetConvar('dw_map', '') end

local function doRestart()
  waiting = false
  local m = mapName()
  -- ensure : relance la map même si elle s'était arrêtée (restart refuse une ressource arrêtée)
  if m ~= '' then ExecuteCommand('ensure ' .. m) end
end

-- 1. les joueurs libèrent la zone (dw:prepare), 2. la map est relancée, 3. ils la rechargent (dw:done)
local function startReload()
  if #GetPlayers() == 0 then doRestart() return end
  waiting = true
  TriggerClientEvent('dw:prepare', -1)
  SetTimeout(4000, function() if waiting then doRestart() end end)
end

RegisterCommand('dw_reload', function(src) if src == 0 then startReload() end end, false)
RegisterNetEvent('dw:askRefresh', function() startReload() end)
RegisterNetEvent('dw:ready', function() if waiting then doRestart() end end)

AddEventHandler('onResourceStart', function(res)
  if res == mapName() then
    SetTimeout(1500, function() TriggerClientEvent('dw:done', -1) end)
  end
end)

RegisterCommand('dw_tp', function(src, args)
  if src ~= 0 then return end
  TriggerClientEvent('dw:tp', -1, tonumber(args[1]) or 0.0, tonumber(args[2]) or 0.0, tonumber(args[3]) or 0.0)
end, false)
"#;

const TOOLS_CLIENT: &str = r#"-- DragoWalker : apparition, noclip, téléportation, coordonnées, rechargement de la zone
local noclip = false
local showCoords = true
local saved = nil

local function notify(msg)
  BeginTextCommandThefeedPost('STRING')
  AddTextComponentSubstringPlayerName(msg)
  EndTextCommandThefeedPostTicker(false, false)
  print('[DragoWalker] ' .. msg)
end

local function spawnPoint()
  local s = GetConvar('dw_spawn', '0 0 72')
  local x, y, z = s:match('([%-%d%.]+)%s+([%-%d%.]+)%s+([%-%d%.]+)')
  return tonumber(x) or 0.0, tonumber(y) or 0.0, tonumber(z) or 72.0
end

local function teleport(x, y, z)
  local ped = PlayerPedId()
  RequestCollisionAtCoord(x, y, z)
  SetEntityCoordsNoOffset(ped, x, y, z, false, false, false)
  local t = GetGameTimer()
  while not HasCollisionLoadedAroundEntity(ped) and GetGameTimer() - t < 5000 do
    RequestCollisionAtCoord(x, y, z)
    Wait(0)
  end
end

-- remet le personnage en vie, santé pleine, sans traces ni effet de mort (FiveM garde le personnage d'une
-- connexion à l'autre : mort avant de se reconnecter, il revenait sans vie)
local FULL = 200
local function revive(x, y, z, h)
  NetworkResurrectLocalPlayer(x, y, z, h or 0.0, true, false)
  local ped = PlayerPedId()
  ClearPedTasksImmediately(ped)
  SetEntityMaxHealth(ped, FULL)
  SetEntityHealth(ped, FULL)
  SetPedArmour(ped, 0)
  ClearPedBloodDamage(ped)
  ResetPedVisibleDamage(ped)
  ClearPlayerWantedLevel(PlayerId())
  AnimpostfxStopAll()
  SetPlayerControl(PlayerId(), true, 0)
  SetEntityVisible(ped, true, false)
  FreezeEntityPosition(ped, false)
  return ped
end

-- apparition (le serveur n'a pas de spawnmanager)
CreateThread(function()
  while not NetworkIsPlayerActive(PlayerId()) do Wait(100) end
  local x, y, z = spawnPoint()
  local model = `mp_m_freemode_01`
  -- dw_tools relancé pendant le jeu (nouvelle version) : déjà en vie avec le bon modèle, on reste sur place
  local cur = PlayerPedId()
  if GetEntityModel(cur) == model and not IsEntityDead(cur) and GetEntityHealth(cur) > 101 then
    local p = GetEntityCoords(cur)
    revive(p.x, p.y, p.z, GetEntityHeading(cur))
    ShutdownLoadingScreen()
    ShutdownLoadingScreenNui()
    DoScreenFadeIn(300)
    return
  end
  RequestModel(model)
  while not HasModelLoaded(model) do Wait(10) end
  SetPlayerModel(PlayerId(), model)
  SetPedDefaultComponentVariation(PlayerPedId())
  SetModelAsNoLongerNeeded(model)
  revive(x, y, z, 0.0)
  teleport(x, y, z)
  ShutdownLoadingScreen()
  ShutdownLoadingScreenNui()
  DoScreenFadeIn(500)
  notify('DragoWalker : F2 noclip, /tp, /tpm, /dw_coords, /dw_refresh')
end)

-- noclip
local function setNoclip(on)
  noclip = on
  local ped = PlayerPedId()
  SetEntityCollision(ped, not on, not on)
  FreezeEntityPosition(ped, on)
  SetEntityVisible(ped, not on, false)
  notify(on and 'Noclip activé' or 'Noclip désactivé')
end

-- on ne meurt jamais : invincible, et ramené au dernier endroit sûr si on tombe dans le vide ou si on meurt
local lastSafe = nil
CreateThread(function()
  while true do
    local ped = PlayerPedId()
    SetEntityInvincible(ped, true)
    SetPlayerInvincible(PlayerId(), true)
    SetPedCanRagdollFromPlayerImpact(ped, false)
    if GetEntityMaxHealth(ped) < FULL then SetEntityMaxHealth(ped, FULL) end
    if GetEntityHealth(ped) < FULL and not IsEntityDead(ped) then SetEntityHealth(ped, FULL) end
    local p = GetEntityCoords(ped)
    if not noclip and not IsPedFalling(ped) and not IsEntityInAir(ped) and not IsPedRagdoll(ped) and not IsEntityDead(ped) then
      lastSafe = p
    end
    local fell = lastSafe ~= nil and not noclip and (p.z < lastSafe.z - 60.0 or p.z < -150.0)
    if IsEntityDead(ped) or IsPlayerDead(PlayerId()) or GetEntityHealth(ped) <= 101 or fell then
      local s = lastSafe or vector3(spawnPoint())
      DoScreenFadeOut(200)
      Wait(250)
      revive(s.x, s.y, s.z + 0.5, GetEntityHeading(ped))
      teleport(s.x, s.y, s.z + 0.5)
      DoScreenFadeIn(300)
      notify(fell and 'Chute dans le vide : retour au dernier endroit sûr' or 'Remis sur pied')
    end
    Wait(250)
  end
end)

RegisterCommand('dw_noclip', function() setNoclip(not noclip) end, false)
RegisterKeyMapping('dw_noclip', 'DragoWalker : noclip', 'keyboard', 'F2')

CreateThread(function()
  while true do
    if noclip then
      local ped = PlayerPedId()
      local p = GetEntityCoords(ped)
      local rot = GetGameplayCamRot(2)
      local h = math.rad(rot.z)
      local pitch = math.rad(rot.x)
      local fx, fy, fz = -math.sin(h) * math.cos(pitch), math.cos(h) * math.cos(pitch), math.sin(pitch)
      local rx, ry = math.cos(h), math.sin(h)
      local speed = IsControlPressed(0, 21) and 3.0 or 0.6
      local dx, dy, dz = 0.0, 0.0, 0.0
      if IsControlPressed(0, 32) then dx, dy, dz = dx + fx, dy + fy, dz + fz end
      if IsControlPressed(0, 33) then dx, dy, dz = dx - fx, dy - fy, dz - fz end
      if IsControlPressed(0, 34) then dx, dy = dx - rx, dy - ry end
      if IsControlPressed(0, 35) then dx, dy = dx + rx, dy + ry end
      if IsControlPressed(0, 22) then dz = dz + 1.0 end
      if IsControlPressed(0, 36) then dz = dz - 1.0 end
      SetEntityCoordsNoOffset(ped, p.x + dx * speed, p.y + dy * speed, p.z + dz * speed, true, true, true)
      SetEntityHeading(ped, rot.z)
      Wait(0)
    else
      Wait(250)
    end
  end
end)

-- coordonnées à l'écran
local function coordsText()
  local ped = PlayerPedId()
  local p = GetEntityCoords(ped)
  return string.format('%.2f, %.2f, %.2f, %.1f', p.x, p.y, p.z, GetEntityHeading(ped))
end

CreateThread(function()
  while true do
    if showCoords then
      SetTextFont(4)
      SetTextScale(0.36, 0.36)
      SetTextColour(255, 255, 255, 230)
      SetTextOutline()
      BeginTextCommandDisplayText('STRING')
      AddTextComponentSubstringPlayerName('x, y, z, cap : ' .. coordsText() .. (noclip and '   (noclip)' or ''))
      EndTextCommandDisplayText(0.015, 0.012)
      Wait(0)
    else
      Wait(300)
    end
  end
end)

RegisterCommand('dw_coords', function() showCoords = not showCoords end, false)
RegisterCommand('dw_copy', function() notify('vector4(' .. coordsText() .. ') — copiable dans la console F8') end, false)

-- téléportation
RegisterCommand('tp', function(_, args)
  local x, y, z = tonumber(args[1]), tonumber(args[2]), tonumber(args[3])
  if x and y then teleport(x + 0.0, y + 0.0, (z or 100.0) + 0.0) end
end, false)

RegisterCommand('tpm', function()
  local blip = GetFirstBlipInfoId(8)
  if not DoesBlipExist(blip) then notify('Pose d\'abord un repère sur la carte') return end
  local c = GetBlipInfoIdCoord(blip)
  for h = 1000.0, 0.0, -25.0 do
    RequestCollisionAtCoord(c.x, c.y, h)
    Wait(0)
    local ok, gz = GetGroundZFor_3dCoord(c.x, c.y, h, false)
    if ok then teleport(c.x, c.y, gz + 1.0) return end
  end
  teleport(c.x, c.y, 100.0)
end, false)

RegisterCommand('dw_home', function() teleport(spawnPoint()) end, false)
RegisterNetEvent('dw:tp', function(x, y, z) teleport(x + 0.0, y + 0.0, z + 0.0) end)

-- rechargement de la zone : le jeu libère collisions et intérieur pendant que la map est relancée
RegisterNetEvent('dw:prepare', function()
  local ped = PlayerPedId()
  saved = GetEntityCoords(ped)
  FreezeEntityPosition(ped, true)
  DoScreenFadeOut(300)
  Wait(300)
  local fy = saved.y > 0.0 and saved.y - 5000.0 or saved.y + 5000.0
  SetFocusPosAndVel(saved.x, fy, 500.0, 0.0, 0.0, 0.0)
  Wait(1500)
  TriggerServerEvent('dw:ready')
end)

RegisterNetEvent('dw:done', function()
  Wait(500)
  ClearFocus()
  local ped = PlayerPedId()
  local p = saved or GetEntityCoords(ped)
  teleport(p.x, p.y, p.z)
  local int = GetInteriorAtCoords(p.x, p.y, p.z)
  if int ~= 0 then RefreshInterior(int) end
  FreezeEntityPosition(ped, noclip)
  DoScreenFadeIn(300)
  notify('Map rechargée')
end)

RegisterCommand('dw_refresh', function() TriggerServerEvent('dw:askRefresh') end, false)
"#;
